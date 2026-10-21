//! Model-written titles for notes you haven't titled yourself.
//!
//! A small local text model behind Ollama's `/api/generate` (default
//! `llama3.2`), run in the embedding worker's batches. Note text never leaves
//! the machine. Two settings keep it light: a 4k context (Ollama's default
//! sized llama3.2 at 10 GB; 4k is 2.6 GB, measured 2026-10-21), and an unload
//! request once the batch is done, so the model is only in memory for seconds.

use serde::Deserialize;
use std::time::Duration;

/// A note needs this much text before its title is worth a model call.
pub const MIN_CHARS: usize = 40;
const MAX_INPUT_CHARS: usize = 3000;

#[derive(Clone, Deserialize)]
pub struct TitleConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_model")]
    pub model: String,
}
fn default_true() -> bool {
    true
}
fn default_url() -> String {
    "http://127.0.0.1:11434".into()
}
fn default_model() -> String {
    "llama3.2".into()
}
impl Default for TitleConfig {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// An auto title is refreshed only when the note has about doubled since it
/// was written, so titles don't shift while a note is being worked on.
pub fn wants_title(source: &str, len: usize, titled_len: Option<usize>) -> bool {
    if len < MIN_CHARS {
        return false;
    }
    match source {
        "derived" => true,
        "auto" => titled_len.map(|t| len >= t * 2).unwrap_or(false),
        _ => false,
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(timeout).build()
}

pub fn generate(cfg: &TitleConfig, body: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Reply {
        response: String,
    }
    let text: String = body.chars().take(MAX_INPUT_CHARS).collect();
    let prompt = format!(
        "Write a title for this note: 3 to 7 words, plain text, no quotes, no trailing period. \
         Name what the note is about, not how it starts.\n\nNOTE:\n{text}\n\nTITLE:"
    );
    let reply: Reply = agent(Duration::from_secs(120))
        .post(&format!("{}/api/generate", cfg.url.trim_end_matches('/')))
        .send_json(serde_json::json!({
            "model": cfg.model, "prompt": prompt, "stream": false, "keep_alive": "30s",
            "options": { "temperature": 0.2, "num_predict": 24, "num_ctx": 4096 }
        }))
        .map_err(|e| format!("No title model at {} ({e})", cfg.url))?
        .into_json()
        .map_err(|e| e.to_string())?;
    clean(&reply.response).ok_or_else(|| "The model returned no usable title.".into())
}

/// Frees the model now instead of after Ollama's idle timer.
pub fn unload(cfg: &TitleConfig) {
    let _ = agent(Duration::from_secs(10))
        .post(&format!("{}/api/generate", cfg.url.trim_end_matches('/')))
        .send_json(serde_json::json!({ "model": cfg.model, "keep_alive": 0 }));
}

fn clean(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.strip_prefix("Title:").or_else(|| line.strip_prefix("TITLE:")).unwrap_or(line).trim();
    let line = line.trim_matches(|c: char| matches!(c, '"' | '\'' | '“' | '”' | '*' | '#' | '`') || c.is_whitespace());
    let line = line.trim_end_matches('.').trim();
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(80).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_model_output() {
        assert_eq!(clean("\"Migrating the Tracker DB.\"\nextra").as_deref(), Some("Migrating the Tracker DB"));
        assert_eq!(clean("  Title: **Q4 Pricing Tiers**").as_deref(), Some("Q4 Pricing Tiers"));
        assert_eq!(clean("\n\n"), None);
    }

    #[test]
    fn titles_once_then_only_when_the_note_doubles() {
        assert!(!wants_title("derived", 10, None));
        assert!(wants_title("derived", 50, None));
        assert!(!wants_title("auto", 150, Some(100)));
        assert!(wants_title("auto", 200, Some(100)));
        assert!(!wants_title("manual", 5000, None));
    }
}
