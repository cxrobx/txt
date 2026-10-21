//! View → Appearance: System, Light or Dark, saved as `appearance` in the
//! support folder. It sets each window's own appearance; the page follows that
//! through `prefers-color-scheme` (main.jsx), so the web side has no setting.

use std::sync::Mutex;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, Theme, Wry};

pub const CHOICES: [(&str, &str); 3] = [("system", "System"), ("light", "Light"), ("dark", "Dark")];
static CURRENT: Mutex<String> = Mutex::new(String::new());

/// The menu's three check items, so a choice can untick the others.
pub struct Items(pub Vec<(&'static str, CheckMenuItem<Wry>)>);

fn file() -> std::path::PathBuf {
    crate::dirs().1.join("appearance")
}

pub fn current() -> String {
    let mut current = CURRENT.lock().unwrap();
    if current.is_empty() {
        let saved = std::fs::read_to_string(file()).unwrap_or_default().trim().to_string();
        *current = if CHOICES.iter().any(|(id, _)| *id == saved) { saved } else { "system".into() };
    }
    current.clone()
}

/// `None` follows macOS.
pub fn theme() -> Option<Theme> {
    match current().as_str() {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

pub fn set(app: &AppHandle, choice: &str) {
    if !CHOICES.iter().any(|(id, _)| *id == choice) {
        return;
    }
    *CURRENT.lock().unwrap() = choice.to_string();
    let _ = std::fs::write(file(), choice);
    if let Some(items) = app.try_state::<Items>() {
        for (id, item) in &items.0 {
            let _ = item.set_checked(*id == choice);
        }
    }
    for window in app.webview_windows().values() {
        let _ = window.set_theme(theme());
    }
}
