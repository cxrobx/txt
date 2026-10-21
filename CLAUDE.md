# txt

Quick notes, one keystroke away. Alfred `txt` opens a new note window; `txt <name>` opens one with that title. Each window has tabs and a ⌘P switcher. Notes are plain files in `~/Documents/scratchpad`.

Tauri 2 (Rust) + React + Tiptap. The editor is lifted from [Margin Notes](https://github.com/cxrobx/Margin) (`renderer/RichText.jsx` and friends). Keep the two looking like one family.

## Layout

| Path | What |
|---|---|
| `src-tauri/src/notes.rs` | The file store: ids, titles, front matter, atomic writes, conflict check. Has the tests. |
| `src-tauri/src/lib.rs` | Commands, windows, menu, `txt://` links, app lifecycle |
| `src/App.jsx` | One window: tabs, saving, menu commands |
| `src/Switcher.jsx` | ⌘P |
| `src/editor/` | Margin's editor, minus attachments, plus `txt://` links and ⌘-click |
| `integrations/alfred/` | The Alfred workflow: `txt-open.sh` (the script) and `build.py` (writes `dist/txt.alfredworkflow`) |

## Notes on disk

- `.md` notes start with front matter: `id`, then `title` and `auto_title: true` when they exist. Other keys are kept as they are.
- **`.txt` notes are never parsed as Markdown.** They open in a plain textarea, so a save writes back only the characters you typed. Their titles live in `~/Library/Application Support/com.cx.txt/index.json`. The test `untitled_txt_keeps_its_uuid_and_bytes` guards this.
- Ids: the front-matter `id`, else the UUID in a TextEdit-era `Untitled-<UUID>.txt` name, else an index entry. Links are `txt://note/<id>`, so renaming a file or changing its title never breaks them.
- A new window creates its file on the first keystroke, so a window opened and closed leaves nothing behind.
- `TXT_NOTES_DIR=<dir>` points a run at another folder. Use it to test against a copy, never the real scratchpad.

## Build, test, install

```sh
export PATH=$HOME/.cargo/bin:$PATH
(cd src-tauri && cargo test)
cargo tauri build --bundles app
rm -rf /Applications/txt.app && cp -R src-tauri/target/release/bundle/macos/txt.app /Applications/
LSR=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
$LSR -u src-tauri/target/release/bundle/macos/txt.app; $LSR -f /Applications/txt.app
```

## Landmines

- **Never build a window inside a link callback.** `on_open_url` and the single-instance callback run inside Tauri's event dispatch, which holds the lock that `WebviewWindowBuilder::build` needs. On a cold launch from `txt://new`, the app hung with no window. Go through `handle_urls_later`.
- **Menu commands use `emit_to(label)`, never `emit`.** `emit` reaches every window, so ⌘W closed a tab in every window.
- **File commands are `async`.** Tauri runs plain commands on the main thread, and the first read of `~/Documents` waits for the macOS privacy prompt, which froze every window until it was answered.
- **Two bundles registered for `txt://`.** LaunchServices learns about the build folder's `txt.app` too, and can route links there. Unregister it after every build (the `-u` line above).
- Each rebuild is ad-hoc signed with a new signature, so macOS may ask about Documents again after a reinstall.
