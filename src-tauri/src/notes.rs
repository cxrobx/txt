//! Notes are plain files in one folder. A `.md` note carries its id and title in
//! a small front-matter block; a `.txt` note is never rewritten into Markdown, so
//! its id and title live in the app's index file instead. Ids are stable across
//! title changes, which is what `txt://note/<id>` links rely on.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

#[derive(Default, Serialize, Deserialize)]
struct Index {
    /// Keyed by file name. Only notes that cannot hold their own id are here.
    #[serde(default)]
    files: HashMap<String, Entry>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Entry {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title_source: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NoteMeta {
    pub id: String,
    pub file: String,
    pub path: String,
    pub kind: String,
    pub title: String,
    /// "manual" (typed or named by you), "auto" (written by the model), or
    /// "derived" (the first line, until something better exists).
    pub title_source: String,
    pub modified: u64,
    pub preview: String,
}

#[derive(Serialize)]
pub struct NoteDoc {
    #[serde(flatten)]
    pub meta: NoteMeta,
    pub body: String,
}

#[derive(Serialize)]
pub struct KeywordHit {
    pub id: String,
    /// The text around the first match, so you can see why it matched.
    pub snippet: String,
}

pub struct Store {
    pub dir: PathBuf,
    index_path: PathBuf,
    index: Mutex<Index>,
}

struct FrontMatter {
    id: Option<String>,
    title: Option<String>,
    auto_title: bool,
    /// Keys this app doesn't own, kept verbatim so a save never drops them.
    other: Vec<String>,
}

const KINDS: [&str; 2] = ["md", "txt"];

impl Store {
    pub fn new(dir: PathBuf, support: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::create_dir_all(&support);
        let index_path = support.join("index.json");
        let index = fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Store { dir, index_path, index: Mutex::new(index) }
    }

    fn save_index(&self, index: &Index) {
        if let Ok(json) = serde_json::to_string_pretty(index) {
            let _ = write_atomic(&self.index_path, &json);
        }
    }

    pub fn list(&self) -> Vec<NoteMeta> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else { return out };
        let mut index = self.index.lock().unwrap();
        let mut dirty = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(meta) = self.meta_for(&path, &mut index, &mut dirty).map(|(m, _)| m) {
                out.push(meta);
            }
        }
        if dirty {
            self.save_index(&index);
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        out
    }

    /// Reads one file and works out its id and title. Returns the body with any
    /// front matter removed.
    fn meta_for(&self, path: &Path, index: &mut Index, dirty: &mut bool) -> Option<(NoteMeta, String)> {
        let file = path.file_name()?.to_str()?.to_string();
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        if file.starts_with('.') || !KINDS.contains(&ext.as_str()) || !path.is_file() {
            return None;
        }
        let raw = fs::read_to_string(path).ok()?;
        let modified = mtime(path);
        let stem = path.file_stem()?.to_str()?.to_string();
        let (fm, body) = if ext == "md" { split_front_matter(&raw) } else { (None, raw.as_str()) };
        let entry = index.files.get(&file).cloned();

        let id = fm
            .as_ref()
            .and_then(|f| f.id.clone())
            .or_else(|| untitled_uuid(&stem))
            .or_else(|| entry.as_ref().map(|e| e.id.clone()))
            .unwrap_or_else(|| {
                let id = uuid::Uuid::new_v4().to_string();
                index.files.insert(file.clone(), Entry { id: id.clone(), ..Default::default() });
                *dirty = true;
                id
            });

        let (title, title_source) = if let Some(t) = fm.as_ref().and_then(|f| f.title.clone()).filter(|t| !t.is_empty()) {
            let auto = fm.as_ref().map(|f| f.auto_title).unwrap_or(false);
            (t, if auto { "auto" } else { "manual" }.to_string())
        } else if let Some(t) = entry.as_ref().and_then(|e| e.title.clone()).filter(|t| !t.is_empty()) {
            (t, entry.as_ref().and_then(|e| e.title_source.clone()).unwrap_or_else(|| "manual".into()))
        } else if untitled_uuid(&stem).is_none() && !stem.starts_with("Untitled") {
            // `txt my plan` named the file; that name is the title.
            (stem.clone(), "manual".to_string())
        } else {
            (first_line_title(body), "derived".to_string())
        };

        let preview = preview(body, title_source == "derived");
        let meta = NoteMeta {
            id,
            file,
            path: path.to_string_lossy().into_owned(),
            kind: ext,
            title,
            title_source,
            modified,
            // A derived title is the first line; don't repeat it underneath.
            preview,
        };
        Some((meta, body.to_string()))
    }

    /// Every note with its body (front matter removed).
    pub fn all(&self) -> Vec<(NoteMeta, String)> {
        let mut index = self.index.lock().unwrap();
        let mut dirty = false;
        let Ok(entries) = fs::read_dir(&self.dir) else { return vec![] };
        let mut out: Vec<_> = entries.flatten().filter_map(|e| self.meta_for(&e.path(), &mut index, &mut dirty)).collect();
        if dirty {
            self.save_index(&index);
        }
        out.sort_by(|a, b| b.0.modified.cmp(&a.0.modified));
        out
    }

    /// Full-text keyword search: every word must appear in the title or body.
    /// Title hits rank first, then the most recently edited.
    pub fn keyword_search(&self, query: &str, limit: usize) -> Vec<KeywordHit> {
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
        if words.is_empty() {
            return vec![];
        }
        let mut hits: Vec<(usize, u64, KeywordHit)> = self
            .all()
            .into_iter()
            .filter_map(|(meta, body)| {
                let title = meta.title.to_lowercase();
                let text = body.to_lowercase();
                if !words.iter().all(|w| title.contains(w.as_str()) || text.contains(w.as_str()) || meta.file.to_lowercase().contains(w.as_str())) {
                    return None;
                }
                let score = words.iter().filter(|w| title.contains(w.as_str())).count();
                let snippet = words.iter().find_map(|w| text.find(w.as_str())).map(|at| around(&body, at)).unwrap_or_default();
                Some((score, meta.modified, KeywordHit { id: meta.id, snippet }))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        hits.into_iter().take(limit).map(|h| h.2).collect()
    }

    fn find(&self, id: &str) -> Result<PathBuf, String> {
        self.list()
            .into_iter()
            .find(|n| n.id == id)
            .map(|n| PathBuf::from(n.path))
            .ok_or_else(|| format!("No note with id {id}"))
    }

    pub fn read(&self, id: &str) -> Result<NoteDoc, String> {
        let path = self.find(id)?;
        let mut index = self.index.lock().unwrap();
        let mut dirty = false;
        let (meta, body) = self.meta_for(&path, &mut index, &mut dirty).ok_or("Unreadable note")?;
        if dirty {
            self.save_index(&index);
        }
        Ok(NoteDoc { meta, body })
    }

    /// `title`: None keeps the current title, Some("") goes back to a derived
    /// one, anything else is a manual title. `auto` marks a model-written title.
    pub fn save(&self, id: &str, body: &str, title: Option<String>, auto: bool, expected_modified: Option<u64>) -> Result<NoteMeta, String> {
        let path = self.find(id)?;
        if let Some(expected) = expected_modified {
            if mtime(&path) != expected {
                return Err("conflict: the file changed on disk since it was opened".into());
            }
        }
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let is_md = path.extension().map(|e| e == "md").unwrap_or(false);
        if is_md {
            let raw = fs::read_to_string(&path).unwrap_or_default();
            let (fm, _) = split_front_matter(&raw);
            let mut fm = fm.unwrap_or(FrontMatter { id: None, title: None, auto_title: false, other: vec![] });
            fm.id = Some(id.to_string());
            if let Some(t) = title {
                let t = t.trim().to_string();
                fm.auto_title = auto && !t.is_empty();
                fm.title = if t.is_empty() { None } else { Some(t) };
            }
            write_atomic(&path, &join_front_matter(&fm, body)).map_err(|e| e.to_string())?;
        } else {
            write_atomic(&path, body).map_err(|e| e.to_string())?;
            if let Some(t) = title {
                let mut index = self.index.lock().unwrap();
                let entry = index.files.entry(file).or_insert_with(|| Entry { id: id.to_string(), ..Default::default() });
                let t = t.trim().to_string();
                entry.title_source = if t.is_empty() { None } else { Some(if auto { "auto" } else { "manual" }.into()) };
                entry.title = if t.is_empty() { None } else { Some(t) };
                self.save_index(&index);
            }
        }
        Ok(self.read(id)?.meta)
    }

    /// Sets a model-written title without touching the note's text. Refused if
    /// the file changed since `expected_modified`, or if you've since given the
    /// note a title of your own.
    pub fn set_auto_title(&self, id: &str, title: &str, expected_modified: u64) -> Result<NoteMeta, String> {
        let doc = self.read(id)?;
        if doc.meta.modified != expected_modified {
            return Err("conflict: the note changed while it was being titled".into());
        }
        if doc.meta.title_source == "manual" {
            return Err("the note has a title of its own".into());
        }
        if doc.meta.kind == "md" {
            return self.save(id, &doc.body, Some(title.to_string()), true, Some(expected_modified));
        }
        {
            let mut index = self.index.lock().unwrap();
            let entry = index.files.entry(doc.meta.file.clone()).or_insert_with(|| Entry { id: id.to_string(), ..Default::default() });
            entry.title = Some(title.to_string());
            entry.title_source = Some("auto".into());
            self.save_index(&index);
        }
        Ok(self.read(id)?.meta)
    }

    pub fn create(&self, name: Option<String>) -> Result<NoteDoc, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let title = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let stem = title.as_deref().map(sanitize).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Untitled-{}", id.to_uppercase()));
        let mut path = self.dir.join(format!("{stem}.md"));
        let mut n = 2;
        while path.exists() {
            path = self.dir.join(format!("{stem} {n}.md"));
            n += 1;
        }
        let fm = FrontMatter { id: Some(id.clone()), title, auto_title: false, other: vec![] };
        write_atomic(&path, &join_front_matter(&fm, "")).map_err(|e| e.to_string())?;
        self.read(&id)
    }

    /// Moves the file to the macOS Trash, where Finder can put it back.
    pub fn trash(&self, id: &str) -> Result<(), String> {
        let path = self.find(id)?;
        let trash = dirs::home_dir().ok_or("No home folder")?.join(".Trash");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut dest = trash.join(&name);
        let mut n = 2;
        while dest.exists() {
            dest = trash.join(format!("{n} {name}"));
            n += 1;
        }
        fs::rename(&path, &dest).map_err(|e| e.to_string())
    }
}

fn mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap().to_string_lossy()));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

/// The id inside TextEdit-era names like `Untitled-01C8BB7D-….txt`.
fn untitled_uuid(stem: &str) -> Option<String> {
    let rest = stem.strip_prefix("Untitled-")?;
    uuid::Uuid::parse_str(rest).ok().map(|u| u.to_string())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | ':' | '\\'))
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .chars()
        .take(120)
        .collect()
}

fn clean_line(line: &str) -> String {
    let mut s = line.trim();
    for prefix in ["#", "-", "*", "+", ">", "[ ]", "[x]", "[X]"] {
        while let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start();
        }
    }
    s.replace("**", "").replace("__", "").replace('`', "")
}

fn first_line_title(body: &str) -> String {
    let line = body.lines().map(clean_line).find(|l| !l.is_empty()).unwrap_or_default();
    if line.is_empty() {
        return "Untitled".into();
    }
    truncate(&line, 60)
}

fn preview(body: &str, skip_first: bool) -> String {
    let text = body.lines().map(clean_line).filter(|l| !l.is_empty()).skip(skip_first as usize).collect::<Vec<_>>().join(" · ");
    truncate(&text, 140)
}

/// ~140 characters of `body` around byte offset `at`, on one line. `at` comes
/// from the lowercased text, which can differ in length, so it is clamped.
fn around(body: &str, at: usize) -> String {
    let at = at.min(body.len());
    let start = body[..body.floor_char_boundary(at)].char_indices().rev().nth(50).map(|(i, _)| i).unwrap_or(0);
    let flat = body[start..].split_whitespace().collect::<Vec<_>>().join(" ");
    let text = truncate(&flat, 140);
    if start > 0 { format!("…{text}") } else { text }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>().trim_end())
    }
}

fn split_front_matter(raw: &str) -> (Option<FrontMatter>, &str) {
    let Some(rest) = raw.strip_prefix("---\n") else { return (None, raw) };
    let Some(end) = rest.find("\n---") else { return (None, raw) };
    let block = &rest[..end];
    let after = &rest[end + 4..];
    let body = after.strip_prefix('\n').unwrap_or(after);
    let body = body.strip_prefix('\n').unwrap_or(body);
    let mut fm = FrontMatter { id: None, title: None, auto_title: false, other: vec![] };
    for line in block.lines() {
        match line.split_once(':') {
            Some(("id", v)) => fm.id = Some(unquote(v)),
            Some(("title", v)) => fm.title = Some(unquote(v)),
            Some(("auto_title", v)) => fm.auto_title = v.trim() == "true",
            _ => fm.other.push(line.to_string()),
        }
    }
    (Some(fm), body)
}

fn join_front_matter(fm: &FrontMatter, body: &str) -> String {
    let mut out = String::from("---\n");
    if let Some(id) = &fm.id {
        out.push_str(&format!("id: {id}\n"));
    }
    if let Some(title) = &fm.title {
        out.push_str(&format!("title: {}\n", quote(title)));
        if fm.auto_title {
            out.push_str("auto_title: true\n");
        }
    }
    for line in &fm.other {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("---\n\n");
    out.push_str(body);
    out
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.starts_with('"') {
        serde_json::from_str(v).unwrap_or_else(|_| v.trim_matches('"').to_string())
    } else {
        v.trim_matches('\'').to_string()
    }
}

/// YAML reads a JSON string as a double-quoted scalar, so quote only when a
/// plain value would be misread.
fn quote(v: &str) -> String {
    let plain = !v.is_empty()
        && !v.contains(": ")
        && !v.contains(" #")
        && !v.starts_with(|c: char| "!&*-?[]{}|>@`\"'%,#".contains(c) || c.is_whitespace())
        && !v.ends_with(':')
        && !["true", "false", "null", "yes", "no"].contains(&v.to_ascii_lowercase().as_str());
    if plain { v.to_string() } else { serde_json::to_string(v).unwrap() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, PathBuf) {
        let root = std::env::temp_dir().join(format!("txt-test-{}", uuid::Uuid::new_v4()));
        let s = Store::new(root.join("notes"), root.join("support"));
        (s, root)
    }

    #[test]
    fn untitled_txt_keeps_its_uuid_and_bytes() {
        let (s, root) = store();
        let id = "01c8bb7d-6901-474b-9fc6-de05756b13ae";
        let path = s.dir.join("Untitled-01C8BB7D-6901-474B-9FC6-DE05756B13AE.txt");
        fs::write(&path, "call *mom* back\n# not a heading").unwrap();
        let note = s.read(id).unwrap();
        assert_eq!(note.meta.kind, "txt");
        assert_eq!(note.meta.title, "call *mom* back");
        s.save(id, "call *mom* back\n# not a heading\nmore", Some("Mom".into()), false, None).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "call *mom* back\n# not a heading\nmore");
        assert_eq!(s.read(id).unwrap().meta.title, "Mom");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_txt_gets_a_stable_id_and_its_name_as_title() {
        let (s, root) = store();
        fs::write(s.dir.join("groceries.txt"), "eggs").unwrap();
        let first = s.list();
        let again = s.list();
        assert_eq!(first[0].id, again[0].id);
        assert_eq!(first[0].title, "groceries");
        assert_eq!(first[0].title_source, "manual");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn md_round_trips_front_matter_and_unknown_keys() {
        let (s, root) = store();
        let doc = s.create(Some("Plan: Q4".into())).unwrap();
        assert_eq!(doc.meta.title, "Plan: Q4");
        assert_eq!(doc.body, "");
        let path = PathBuf::from(&doc.meta.path);
        let raw = fs::read_to_string(&path).unwrap().replace("---\n\n", "tags: [a]\n---\n\n");
        fs::write(&path, raw).unwrap();
        s.save(&doc.meta.id, "# Hello\n\nworld", None, false, None).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("title: \"Plan: Q4\"") && raw.contains("tags: [a]"), "{raw}");
        let read = s.read(&doc.meta.id).unwrap();
        assert_eq!(read.body, "# Hello\n\nworld");
        s.save(&doc.meta.id, "# Hello", Some("Auto one".into()), true, None).unwrap();
        let meta = s.read(&doc.meta.id).unwrap().meta;
        assert_eq!((meta.title.as_str(), meta.title_source.as_str()), ("Auto one", "auto"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn untitled_md_derives_title_from_first_line() {
        let (s, root) = store();
        let doc = s.create(None).unwrap();
        assert_eq!(doc.meta.title, "Untitled");
        let meta = s.save(&doc.meta.id, "- [ ] **buy** milk\nand bread", None, false, None).unwrap();
        assert_eq!((meta.title.as_str(), meta.title_source.as_str()), ("buy milk", "derived"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keyword_search_reads_the_whole_body() {
        let (s, root) = store();
        fs::write(s.dir.join("a.txt"), format!("first line\n{}\nthe Zebra crossing", "filler ".repeat(100))).unwrap();
        fs::write(s.dir.join("zebra notes.txt"), "nothing").unwrap();
        let hits = s.keyword_search("zebra", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(s.read(&hits[0].id).unwrap().meta.title, "zebra notes", "title hits rank first");
        assert!(hits[1].snippet.contains("Zebra crossing"), "{}", hits[1].snippet);
        assert!(s.keyword_search("zebra missing", 10).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auto_title_never_rewrites_text_or_beats_a_manual_title() {
        let (s, root) = store();
        let path = s.dir.join("Untitled-01C8BB7D-6901-474B-9FC6-DE05756B13AE.txt");
        fs::write(&path, "  raw *text*\n").unwrap();
        let id = "01c8bb7d-6901-474b-9fc6-de05756b13ae";
        let before = s.read(id).unwrap().meta;
        let meta = s.set_auto_title(id, "Raw Text Note", before.modified).unwrap();
        assert_eq!((meta.title.as_str(), meta.title_source.as_str()), ("Raw Text Note", "auto"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "  raw *text*\n");
        assert!(s.set_auto_title(id, "x", before.modified + 1).unwrap_err().starts_with("conflict"));

        let doc = s.create(Some("Mine".into())).unwrap();
        assert!(s.set_auto_title(&doc.meta.id, "Theirs", doc.meta.modified).is_err());
        let md = s.create(None).unwrap();
        let saved = s.save(&md.meta.id, "# body\n\ntext", None, false, None).unwrap();
        let meta = s.set_auto_title(&md.meta.id, "Body Text", saved.modified).unwrap();
        assert_eq!(meta.title_source, "auto");
        assert_eq!(s.read(&md.meta.id).unwrap().body, "# body\n\ntext");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_save_is_refused() {
        let (s, root) = store();
        let doc = s.create(None).unwrap();
        assert!(s.save(&doc.meta.id, "x", None, false, Some(1)).unwrap_err().starts_with("conflict"));
        fs::remove_dir_all(root).unwrap();
    }
}
