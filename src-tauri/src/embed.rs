//! Meaning-based search over the notes, embedded in batches.
//!
//! The model is a separate local service with Ollama's `/api/embed` API (Ollama
//! itself, or anything that speaks it). It is the only heavy piece, and these
//! services unload an idle model, so txt asks for embeddings rarely: a save only
//! marks the note, and the worker embeds every changed note in one batch once
//! you have stopped typing for `idle_secs`, or when a window closes. Without a
//! reachable model nothing is marked done and search stays keyword-only.

use crate::notes::Store;
use crate::titles::{self, TitleConfig};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const BATCH: usize = 32;
/// A related note must score within this much of the best match, and above
/// FLOOR. Calibrated on EmbeddingGemma 2 (2026-10-21): real matches sat at
/// 0.64–0.78, and unrelated notes in a cluster just below the best few.
const SPREAD: f32 = 0.12;
const FLOOR: f32 = 0.60;
const CHUNK_CHARS: usize = 1200;
const MAX_CHUNKS: usize = 40;
/// Notes changed outside txt (another editor, an agent) are picked up this often.
const RESCAN: Duration = Duration::from_secs(300);

#[derive(Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_idle")]
    pub idle_secs: u64,
    #[serde(default)]
    pub titles: TitleConfig,
}
fn default_url() -> String {
    "http://127.0.0.1:11434".into()
}
fn default_model() -> String {
    "embeddinggemma".into()
}
fn default_idle() -> u64 {
    180
}

impl Config {
    /// `config.json` in the app's support folder; every key is optional.
    pub fn load(support: &Path) -> Self {
        std::fs::read_to_string(support.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::from_str("{}").unwrap())
    }

    /// Each model family wants its own task prefixes; a wrong one quietly
    /// worsens results instead of failing. These match vault-mcp's table.
    fn document(&self, title: &str, text: &str) -> String {
        let m = self.model.split(':').next().unwrap_or("");
        if m.starts_with("embeddinggemma") {
            format!("title: {title} | text: {text}")
        } else if m == "nomic-embed-text" {
            format!("search_document: {title}\n{text}")
        } else {
            format!("{title}\n{text}")
        }
    }
    fn query(&self, q: &str) -> String {
        let m = self.model.split(':').next().unwrap_or("");
        if m.starts_with("embeddinggemma") {
            format!("task: search result | query: {q}")
        } else if m == "nomic-embed-text" {
            format!("search_query: {q}")
        } else {
            q.to_string()
        }
    }
}

#[derive(Clone, Serialize, Default)]
pub struct Status {
    pub model: String,
    pub available: bool,
    pub indexed: usize,
    pub total: usize,
    pub error: Option<String>,
    pub titles_error: Option<String>,
}

#[derive(Serialize)]
pub struct Hit {
    pub id: String,
    pub score: f32,
    pub snippet: String,
}

pub struct Index {
    conn: Mutex<Connection>,
    pub cfg: Config,
    /// Note id → when it was last saved in txt.
    edits: Mutex<HashMap<String, Instant>>,
    flush: AtomicBool,
    status: Mutex<Status>,
}

impl Index {
    pub fn open(support: &Path, cfg: Config) -> Self {
        let conn = Connection::open(support.join("index.db")).expect("open index.db");
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS notes (id TEXT PRIMARY KEY, hash TEXT NOT NULL, model TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS chunks (id TEXT NOT NULL, idx INTEGER NOT NULL, text TEXT NOT NULL, vec BLOB NOT NULL, PRIMARY KEY (id, idx));
             CREATE TABLE IF NOT EXISTS titles (id TEXT PRIMARY KEY, len INTEGER NOT NULL);",
        )
        .expect("create index tables");
        let status = Status { model: cfg.model.clone(), ..Default::default() };
        Index { conn: Mutex::new(conn), cfg, edits: Mutex::new(HashMap::new()), flush: AtomicBool::new(false), status: Mutex::new(status) }
    }

    pub fn touch(&self, id: &str) {
        self.edits.lock().unwrap().insert(id.to_string(), Instant::now());
    }

    /// Embed what's pending at the next tick, without waiting out the idle time.
    pub fn flush_soon(&self) {
        self.flush.store(true, Ordering::SeqCst);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    fn embed(&self, inputs: &[String], timeout: Duration) -> Result<Vec<Vec<f32>>, String> {
        #[derive(Deserialize)]
        struct Reply {
            embeddings: Vec<Vec<f32>>,
        }
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(timeout).build();
        let reply: Reply = agent
            .post(&format!("{}/api/embed", self.cfg.url.trim_end_matches('/')))
            .send_json(serde_json::json!({ "model": self.cfg.model, "input": inputs }))
            .map_err(|e| match e {
                ureq::Error::Status(code, r) => format!("{} answered {code}: {}", self.cfg.url, r.into_string().unwrap_or_default()),
                other => format!("No embedding model at {} ({other})", self.cfg.url),
            })?
            .into_json()
            .map_err(|e| e.to_string())?;
        if reply.embeddings.len() != inputs.len() {
            return Err("The embedding service returned the wrong number of vectors.".into());
        }
        Ok(reply.embeddings.into_iter().map(normalize).collect())
    }

    /// Titles the notes that want one (see `titles::wants_title`), skipping
    /// any still being edited. Stops at the first error: if the model isn't
    /// there, one failed request per batch is the whole cost.
    fn retitle(&self, store: &Store, recent: &HashSet<String>, on_title: &dyn Fn(&str)) {
        let cfg = &self.cfg.titles;
        if !cfg.enabled {
            return;
        }
        let titled: HashMap<String, usize> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id, len FROM titles").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))).unwrap();
            rows.flatten().collect()
        };
        let mut used = false;
        let mut error = None;
        for (meta, body) in store.all() {
            let len = body.trim().chars().count();
            if recent.contains(&meta.id) || !titles::wants_title(&meta.title_source, len, titled.get(&meta.id).copied()) {
                continue;
            }
            used = true;
            match titles::generate(cfg, &body) {
                Ok(title) => {
                    // Refused if you edited or titled the note meanwhile; it's
                    // simply looked at again next batch.
                    if store.set_auto_title(&meta.id, &title, meta.modified).is_ok() {
                        let _ = self.conn.lock().unwrap().execute(
                            "INSERT INTO titles (id, len) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET len = ?2",
                            params![meta.id, len as i64],
                        );
                        on_title(&meta.id);
                    }
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        if used && error.is_none() {
            titles::unload(cfg);
        }
        if used {
            self.status.lock().unwrap().titles_error = error;
        }
    }

    /// Brings the index in line with the folder. Notes edited in the last
    /// `idle_secs` wait (unless `all`), so typing never wakes the model.
    pub fn sync(&self, store: &Store, all: bool, on_title: &dyn Fn(&str)) {
        let idle = Duration::from_secs(self.cfg.idle_secs);
        let recent: HashSet<String> = if all {
            HashSet::new()
        } else {
            self.edits.lock().unwrap().iter().filter(|(_, t)| t.elapsed() < idle).map(|(id, _)| id.clone()).collect()
        };
        // Titles first: the title is part of what gets embedded, so titling
        // afterwards would embed each newly titled note twice.
        self.retitle(store, &recent, on_title);
        let docs = store.all();
        let known: HashMap<String, (String, String)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id, hash, model FROM notes").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))).unwrap();
            rows.flatten().collect()
        };
        // Forget notes that left the folder.
        let present: HashSet<&str> = docs.iter().map(|(m, _)| m.id.as_str()).collect();
        {
            let conn = self.conn.lock().unwrap();
            for id in known.keys().filter(|id| !present.contains(id.as_str())) {
                let _ = conn.execute("DELETE FROM notes WHERE id = ?1", params![id]);
                let _ = conn.execute("DELETE FROM chunks WHERE id = ?1", params![id]);
            }
        }
        let stale: Vec<_> = docs
            .iter()
            .filter(|(m, body)| {
                let hash = content_hash(&m.title, body);
                known.get(&m.id).map(|(h, model)| h != &hash || model != &self.cfg.model).unwrap_or(true) && !recent.contains(&m.id)
            })
            .collect();

        let mut error = None;
        if !stale.is_empty() {
            // One request per BATCH chunks, across notes.
            let mut jobs: Vec<(usize, usize, String, String)> = Vec::new(); // (doc, idx, chunk, input)
            for (n, (meta, body)) in stale.iter().enumerate() {
                // An empty note's vector is just its title, which loosely matches
                // every query; it is recorded with no chunks instead.
                if body.trim().is_empty() {
                    continue;
                }
                for (i, chunk) in chunks(body).into_iter().enumerate() {
                    let input = self.cfg.document(&meta.title, &chunk);
                    jobs.push((n, i, chunk, input));
                }
            }
            let mut vectors = Vec::with_capacity(jobs.len());
            for batch in jobs.chunks(BATCH) {
                let inputs: Vec<String> = batch.iter().map(|j| j.3.clone()).collect();
                match self.embed(&inputs, Duration::from_secs(180)) {
                    Ok(v) => vectors.extend(v),
                    Err(e) => {
                        error = Some(e);
                        break;
                    }
                }
            }
            if error.is_none() {
                let mut conn = self.conn.lock().unwrap();
                let tx = conn.transaction().unwrap();
                for (n, (meta, body)) in stale.iter().enumerate() {
                    tx.execute("DELETE FROM chunks WHERE id = ?1", params![meta.id]).unwrap();
                    for ((_, idx, text, _), vec) in jobs.iter().zip(&vectors).filter(|((d, ..), _)| *d == n) {
                        tx.execute("INSERT INTO chunks (id, idx, text, vec) VALUES (?1, ?2, ?3, ?4)", params![meta.id, *idx as i64, text, to_blob(vec)]).unwrap();
                    }
                    tx.execute(
                        "INSERT INTO notes (id, hash, model) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET hash = ?2, model = ?3",
                        params![meta.id, content_hash(&meta.title, body), self.cfg.model],
                    )
                    .unwrap();
                }
                tx.commit().unwrap();
            }
        }

        let indexed: usize = self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM notes WHERE model = ?1", params![self.cfg.model], |r| r.get(0))
            .unwrap_or(0);
        let mut status = self.status.lock().unwrap();
        status.indexed = indexed;
        status.total = docs.len();
        if !stale.is_empty() {
            status.available = error.is_none();
            status.error = error;
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>, String> {
        let q = self.embed(&[self.cfg.query(query)], Duration::from_secs(30)).map_err(|e| {
            let mut s = self.status.lock().unwrap();
            s.available = false;
            s.error = Some(e.clone());
            e
        })?;
        self.status.lock().unwrap().available = true;
        let q = &q[0];
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT c.id, c.text, c.vec FROM chunks c JOIN notes n ON n.id = c.id WHERE n.model = ?1")
            .map_err(|e| e.to_string())?;
        let mut best: HashMap<String, (f32, String)> = HashMap::new();
        let rows = stmt
            .query_map(params![self.cfg.model], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?)))
            .map_err(|e| e.to_string())?;
        for (id, text, blob) in rows.flatten() {
            let v = from_blob(&blob);
            if v.len() != q.len() {
                continue;
            }
            let score: f32 = v.iter().zip(q).map(|(a, b)| a * b).sum();
            if best.get(&id).map(|(s, _)| score > *s).unwrap_or(true) {
                best.insert(id, (score, text));
            }
        }
        let mut hits: Vec<Hit> = best.into_iter().map(|(id, (score, text))| Hit { id, score, snippet: snippet(&text) }).collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        let cut = hits.first().map(|h| (h.score - SPREAD).max(FLOOR)).unwrap_or(FLOOR);
        hits.retain(|h| h.score >= cut);
        hits.truncate(limit);
        Ok(hits)
    }
}

/// The worker: one quiet thread that wakes every few seconds and embeds only
/// when there is a reason to.
pub fn run(app: AppHandle) {
    let mut last_scan: Option<Instant> = None;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let index = app.state::<Index>();
        let store = app.state::<Store>();
        let idle = Duration::from_secs(index.cfg.idle_secs);
        let flush = index.flush.swap(false, Ordering::SeqCst);
        let settled = {
            let mut edits = index.edits.lock().unwrap();
            let due = edits.values().any(|t| t.elapsed() >= idle);
            if flush || due {
                // These are about to be embedded; anything still being typed stays.
                edits.retain(|_, t| !flush && t.elapsed() < idle);
            }
            due
        };
        let rescan = last_scan.map(|t| t.elapsed() >= RESCAN).unwrap_or(true);
        if flush || settled || rescan {
            // Open windows refresh the title in their tabs and switcher.
            let notify = |id: &str| {
                let _ = app.emit("notes-changed", serde_json::json!({ "id": id, "by": "titler" }));
            };
            index.sync(&store, flush, &notify);
            last_scan = Some(Instant::now());
        }
    }
}

fn chunks(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in body.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if !cur.is_empty() && cur.len() + para.len() > CHUNK_CHARS {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
        while cur.len() > CHUNK_CHARS * 2 {
            let cut = cur.char_indices().map(|(i, _)| i).take_while(|&i| i <= CHUNK_CHARS).last().unwrap_or(0).max(1);
            let cut = (cut..=cur.len()).find(|&i| cur.is_char_boundary(i)).unwrap_or(cur.len());
            out.push(cur[..cut].to_string());
            cur = cur[cut..].to_string();
        }
    }
    if !cur.trim().is_empty() || out.is_empty() {
        out.push(cur);
    }
    out.truncate(MAX_CHUNKS);
    out
}

fn snippet(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 160 {
        format!("{}…", flat.chars().take(160).collect::<String>().trim_end())
    } else {
        flat
    }
}

/// FNV-1a: stable across Rust versions, unlike `DefaultHasher`.
fn content_hash(title: &str, body: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in title.bytes().chain([0u8]).chain(body.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}
fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_keep_paragraphs_and_cap_size() {
        assert_eq!(chunks(""), vec![String::new()]);
        assert_eq!(chunks("a\n\nb"), vec!["a\n\nb".to_string()]);
        let long = "word ".repeat(1000);
        let parts = chunks(&long);
        assert!(parts.len() >= 2 && parts.iter().all(|p| p.len() <= CHUNK_CHARS * 2), "{:?}", parts.iter().map(|p| p.len()).collect::<Vec<_>>());
        let emoji = "é😀".repeat(900);
        assert!(!chunks(&emoji).is_empty());
    }

    #[test]
    fn prefixes_follow_the_model() {
        let mut cfg: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.query("x"), "task: search result | query: x");
        cfg.model = "embeddinggemma-2".into();
        assert_eq!(cfg.document("T", "b"), "title: T | text: b");
        cfg.model = "nomic-embed-text:latest".into();
        assert_eq!(cfg.query("x"), "search_query: x");
        cfg.model = "mxbai".into();
        assert_eq!(cfg.query("x"), "x");
    }

    #[test]
    fn hash_and_blob_round_trip() {
        assert_eq!(content_hash("a", "b"), content_hash("a", "b"));
        assert_ne!(content_hash("a", "b"), content_hash("ab", ""));
        let v = normalize(vec![3.0, 4.0]);
        assert_eq!(from_blob(&to_blob(&v)), v);
    }
}
