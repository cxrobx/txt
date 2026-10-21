pub mod embed;
pub mod mcp;
pub mod notes;
mod titles;

use embed::{Hit, Index, Status};
use notes::{KeywordHit, NoteDoc, NoteMeta, Store};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, RunEvent, TitleBarStyle, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;

static WINDOWS: AtomicUsize = AtomicUsize::new(0);
/// Something was handed to txt at launch, so the blank fallback window stays shut.
static OPENING_FILE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The window menu commands go to. Tracked from focus events, because a window
/// that was just opened doesn't report `is_focused` yet.
static LAST_FOCUSED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

#[derive(Clone, serde::Serialize)]
struct Changed {
    id: String,
    by: String,
}

fn changed(app: &AppHandle, id: &str, by: &str) {
    let _ = app.emit("notes-changed", Changed { id: id.into(), by: by.into() });
}

/// Notices edits made outside this app (an agent through txt-mcp, another
/// editor, a sync) and tells open windows, which reload any tab with nothing
/// unsaved. Polls modified times once a second: ~150 `stat` calls, no reads,
/// and no extra dependency. Saves from txt itself show up here too, and are
/// ignored by the windows because the tab already has that modified time.
fn watch(app: AppHandle) {
    let store = app.state::<Store>();
    let mut seen = store.stamps();
    // File name → id, so a deleted file can still be named to the windows.
    let mut ids: std::collections::HashMap<String, String> = store.list().into_iter().map(|n| (n.file, n.id)).collect();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let now = store.stamps();
        let moved: Vec<String> = now
            .iter()
            .filter(|(f, t)| seen.get(*f) != Some(t))
            .map(|(f, _)| f.clone())
            .chain(seen.keys().filter(|f| !now.contains_key(*f)).cloned())
            .collect();
        seen = now;
        if moved.is_empty() {
            continue;
        }
        let fresh: std::collections::HashMap<String, String> = store.list().into_iter().map(|n| (n.file, n.id)).collect();
        for file in moved {
            if let Some(id) = fresh.get(&file).or_else(|| ids.get(&file)) {
                app.state::<Index>().touch(id);
                changed(&app, id, "disk");
            }
        }
        ids = fresh;
    }
}

// File commands are async so Tauri runs them off the main thread: a slow disk
// or a macOS privacy prompt for ~/Documents must not freeze every window.
#[tauri::command]
async fn list_notes(store: tauri::State<'_, Store>) -> Result<Vec<NoteMeta>, String> {
    Ok(store.list())
}

#[tauri::command]
async fn read_note(store: tauri::State<'_, Store>, id: String) -> Result<NoteDoc, String> {
    store.read(&id)
}

#[tauri::command]
async fn create_note(app: AppHandle, window: WebviewWindow, store: tauri::State<'_, Store>, index: tauri::State<'_, Index>, name: Option<String>) -> Result<NoteDoc, String> {
    let doc = store.create(name)?;
    index.touch(&doc.meta.id);
    changed(&app, &doc.meta.id, window.label());
    Ok(doc)
}

#[tauri::command]
async fn save_note(
    app: AppHandle,
    window: WebviewWindow,
    store: tauri::State<'_, Store>,
    index: tauri::State<'_, Index>,
    id: String,
    body: String,
    title: Option<String>,
    expected_modified: Option<u64>,
) -> Result<NoteMeta, String> {
    let meta = store.save(&id, &body, title, false, expected_modified)?;
    index.touch(&id);
    changed(&app, &id, window.label());
    Ok(meta)
}

#[tauri::command]
async fn trash_note(app: AppHandle, window: WebviewWindow, store: tauri::State<'_, Store>, id: String) -> Result<(), String> {
    store.trash(&id)?;
    changed(&app, &id, window.label());
    Ok(())
}

#[tauri::command]
async fn search_keyword(store: tauri::State<'_, Store>, query: String) -> Result<Vec<KeywordHit>, String> {
    Ok(store.keyword_search(&query, 60))
}

/// Can take a few seconds when the model has to load; the switcher shows
/// keyword results meanwhile.
#[tauri::command]
async fn search_semantic(index: tauri::State<'_, Index>, query: String) -> Result<Vec<Hit>, String> {
    index.search(&query, 8)
}

#[tauri::command]
fn embed_status(index: tauri::State<'_, Index>) -> Status {
    index.status()
}

/// A closing window's edits are done; embed them without waiting out the idle time.
#[tauri::command]
fn flush_embeddings(index: tauri::State<'_, Index>) {
    index.flush_soon();
}

#[tauri::command]
async fn open_window(app: AppHandle, note: Option<String>) -> Result<(), String> {
    new_window(&app, note.as_deref(), None).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("/usr/bin/pbcopy").stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    child.stdin.take().unwrap().write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn reveal(store: tauri::State<'_, Store>, id: String) -> Result<(), String> {
    let path = store.read(&id)?.meta.path;
    std::process::Command::new("/usr/bin/open").args(["-R", &path]).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Opens links the editor doesn't handle itself. Documents go to Onyx when it
/// is installed, as they do from Margin; everything else to the app that owns
/// the scheme.
#[tauri::command]
fn open_external(target: String) -> Result<(), String> {
    let home = dirs::home_dir().unwrap_or_default();
    let path = if let Some(rest) = target.strip_prefix("file://") {
        Some(url::Url::parse(&format!("file://{rest}")).ok().and_then(|u| u.to_file_path().ok()).ok_or("Invalid file link.")?)
    } else if let Some(rest) = target.strip_prefix("~/") {
        Some(home.join(rest))
    } else if target.starts_with('/') {
        Some(std::path::PathBuf::from(&target))
    } else {
        None
    };
    let mut cmd = std::process::Command::new("/usr/bin/open");
    if let Some(path) = path {
        if !path.is_file() {
            return Err("This document has moved or is no longer available.".into());
        }
        let onyx = std::path::Path::new("/Applications/Onyx.app");
        if onyx.exists() {
            cmd.arg("-a").arg(onyx);
        }
        cmd.arg(path);
    } else {
        let allowed = ["http://", "https://", "mailto:", "obsidian://", "cxtasks://", "cxboards://", "margin://"];
        if !allowed.iter().any(|p| target.starts_with(p)) {
            return Err("This link type is not supported.".into());
        }
        cmd.arg(&target);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// `note`: open that note. `name`: a new note that will be created with that
/// name on the first keystroke. Neither: a new untitled note, also created
/// lazily, so a window opened and closed leaves no empty file behind.
fn new_window(app: &AppHandle, note: Option<&str>, name: Option<&str>) -> tauri::Result<WebviewWindow> {
    let n = WINDOWS.fetch_add(1, Ordering::SeqCst);
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    match (note, name) {
        (Some(id), _) => query.append_pair("note", id),
        (None, Some(name)) => query.append_pair("name", name),
        _ => query.append_pair("new", "1"),
    };
    let mut builder = WebviewWindowBuilder::new(app, format!("w-{n}"), WebviewUrl::App(format!("index.html?{}", query.finish()).into()))
        .title("txt")
        .inner_size(620.0, 680.0)
        .min_inner_size(360.0, 260.0)
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(14.0, 20.0))
        .disable_drag_drop_handler();
    // Cascade from the window you're in, like TextEdit.
    if let Some(prev) = focused(app) {
        if let (Ok(pos), Ok(scale)) = (prev.outer_position(), prev.scale_factor()) {
            let p = pos.to_logical::<f64>(scale);
            builder = builder.position(p.x + 26.0, p.y + 26.0);
        }
    } else {
        builder = builder.center();
    }
    let window = builder.build()?;
    *LAST_FOCUSED.lock().unwrap() = window.label().to_string();
    let _ = window.set_focus();
    Ok(window)
}

fn focused(app: &AppHandle) -> Option<WebviewWindow> {
    let label = LAST_FOCUSED.lock().unwrap().clone();
    app.get_webview_window(&label)
        .or_else(|| app.webview_windows().into_values().find(|w| w.is_focused().unwrap_or(false)))
}

/// Link callbacks run inside Tauri's event dispatch, which holds the listener
/// lock that building a window needs, so a window opened there deadlocks the
/// app (seen on a cold launch from `txt://new`). Queue the work instead.
/// The last URL handled and when. macOS reports a file handed to the app
/// twice (as a URL and as a file), so a repeat within a second is dropped.
static LAST_URL: std::sync::Mutex<Option<(String, std::time::Instant)>> = std::sync::Mutex::new(None);

fn handle_urls_later(app: &AppHandle, urls: Vec<String>) {
    let urls: Vec<String> = urls
        .into_iter()
        .filter(|u| {
            let mut last = LAST_URL.lock().unwrap();
            let repeat = matches!(&*last, Some((prev, at)) if prev == u && at.elapsed() < std::time::Duration::from_secs(1));
            *last = Some((u.clone(), std::time::Instant::now()));
            !repeat
        })
        .collect();
    let app = app.clone();
    std::thread::spawn(move || {
        // A file handed to txt (Finder's Open With, `open -b com.cx.txt <file>`,
        // a search launcher) opens as its note, if it's one of txt's notes.
        // Resolved here, off the main thread, because it reads the folder.
        let urls: Vec<String> = urls.into_iter().filter_map(|u| if u.starts_with("file:") { note_link_for_file(&app, &u) } else { Some(u) }).collect();
        let h = app.clone();
        let _ = app.run_on_main_thread(move || {
            for url in urls {
                handle_url(&h, &url);
            }
        });
    });
}

fn note_link_for_file(app: &AppHandle, raw: &str) -> Option<String> {
    let path = url::Url::parse(raw).ok()?.to_file_path().ok()?.canonicalize().ok()?;
    let note = app.state::<Store>().list().into_iter().find(|n| std::path::Path::new(&n.path).canonicalize().ok().as_deref() == Some(path.as_path()))?;
    Some(format!("txt://note/{}", note.id))
}

/// `txt://new`, `txt://new?name=plan`, `txt://note/<id>`.
fn handle_url(app: &AppHandle, raw: &str) {
    let Ok(url) = url::Url::parse(raw) else { return };
    if url.scheme() != "txt" {
        return;
    }
    let name = url.query_pairs().find(|(k, _)| k == "name").map(|(_, v)| v.into_owned()).filter(|v| !v.trim().is_empty());
    match url.host_str() {
        Some("note") => {
            let id = url.path().trim_matches('/').to_string();
            if !id.is_empty() {
                let _ = new_window(app, Some(&id), None);
            }
        }
        _ => {
            let _ = new_window(app, None, name.as_deref());
        }
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let item = |id: &str, label: &str, accel: &str| MenuItem::with_id(app, id, label, true, Some(accel));
    let app_menu = Submenu::with_items(
        app,
        "txt",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About txt"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item("new-window", "New Note", "CmdOrCtrl+N")?,
            &item("new-tab", "New Tab", "CmdOrCtrl+T")?,
            &item("switch", "Open Note…", "CmdOrCtrl+P")?,
            &PredefinedMenuItem::separator(app)?,
            &item("close-tab", "Close Tab", "CmdOrCtrl+W")?,
            &item("close-window", "Close Window", "CmdOrCtrl+Shift+W")?,
            &PredefinedMenuItem::separator(app)?,
            &item("copy-link", "Copy Link to Note", "CmdOrCtrl+Shift+C")?,
            &item("reveal", "Show in Finder", "CmdOrCtrl+Shift+R")?,
            &item("trash", "Move to Trash", "CmdOrCtrl+Backspace")?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &item("next-tab", "Show Next Tab", "Ctrl+Tab")?,
            &item("prev-tab", "Show Previous Tab", "Ctrl+Shift+Tab")?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &window])
}

/// The notes folder and the support folder (index, embeddings, config), shared
/// by the app and `txt-mcp`.
pub fn dirs() -> (std::path::PathBuf, std::path::PathBuf) {
    let home = dirs::home_dir().expect("home folder");
    let notes_dir = std::env::var("TXT_NOTES_DIR").map(Into::into).unwrap_or_else(|_| home.join("Documents/scratchpad"));
    let support = std::env::var("TXT_SUPPORT_DIR").map(Into::into).unwrap_or_else(|_| home.join("Library/Application Support/com.cx.txt"));
    (notes_dir, support)
}

pub fn run() {
    let (notes_dir, support) = dirs();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _| {
            handle_urls_later(app, argv.into_iter().filter(|a| a.starts_with("txt://")).collect());
        }))
        .plugin(tauri_plugin_deep_link::init())
        .manage(Store::new(notes_dir, support.clone()))
        .manage(Index::open(&support, embed::Config::load(&support)))
        .menu(|app| menu(app))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(true) = event {
                *LAST_FOCUSED.lock().unwrap() = window.label().to_string();
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-window" => {
                let _ = new_window(app, None, None);
            }
            "close-window" => {
                if let Some(w) = focused(app) {
                    let _ = app.emit_to(w.label(), "menu", "close-window");
                }
            }
            id => {
                // `emit` would reach every window; menu commands go to one.
                if let Some(w) = focused(app) {
                    let _ = app.emit_to(w.label(), "menu", id);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![list_notes, read_note, create_note, save_note, trash_note, open_window, open_external, copy_text, reveal, search_keyword, search_semantic, embed_status, flush_embeddings])
        .setup(|app| {
            let worker = app.handle().clone();
            std::thread::spawn(move || embed::run(worker));
            let watcher = app.handle().clone();
            std::thread::spawn(move || watch(watcher));
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                handle_urls_later(&handle, event.urls().iter().map(|u| u.to_string()).collect());
            });
            // What launched the app (a file or link) when it wasn't running.
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                OPENING_FILE.store(true, Ordering::SeqCst);
                handle_urls_later(app.handle(), urls.iter().map(|u| u.to_string()).collect());
            }
            // A cold launch from `txt://…` delivers the URL just after setup.
            // Open a blank note only if nothing else opened a window by then.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let h = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if h.webview_windows().is_empty() && !OPENING_FILE.load(Ordering::SeqCst) {
                        let _ = new_window(&h, None, None);
                    }
                });
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building txt");

    app.run(|app, event| match event {
        // Stay running with no windows open, so the next `txt` is instant.
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Reopen { has_visible_windows: false, .. } => {
            let _ = new_window(app, None, None);
        }
        _ => {}
    });
}
