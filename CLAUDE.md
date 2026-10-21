# txt

Quick notes, one keystroke away. Alfred `txt` opens a new note window; `txt <name>` opens one with that title. Each window has tabs and a ⌘P switcher. Notes are plain files in `~/Documents/scratchpad`.

Tauri 2 (Rust) + React + Tiptap. The editor is lifted from [Margin Notes](https://github.com/cxrobx/Margin) (`renderer/RichText.jsx` and friends). Keep the two looking like one family.

## Layout

| Path | What |
|---|---|
| `src-tauri/src/notes.rs` | The file store: ids, titles, front matter, atomic writes, conflict check. Has the tests. |
| `src-tauri/src/lib.rs` | Commands, windows, menu, `txt://` links, app lifecycle |
| `src/App.jsx` | One window: tabs, saving, menu commands |
| `src-tauri/src/embed.rs` | Meaning search: the batch worker, `index.db`, per-model prefixes, the score cut-off |
| `src-tauri/src/titles.rs` | Auto titles: when a note gets one, the prompt, cleaning the model's answer |
| `src-tauri/src/mcp.rs` | The MCP tools; `src/bin/txt-mcp.rs` is its stdio entry point, bundled as `Contents/MacOS/txt-mcp` |
| `src/TabBar.jsx` | Onyx's tab bar (`ask-widget/src/onyx/tabs_ui.py`), minus auto-hide and split view: 180→88 px pills, overflow into the ⌄ list, hover-widen with scrolling titles, drag to reorder. Widths and `hidden` are set on the DOM, never through React props |
| `src-tauri/src/closed.rs` + `src/recents.mjs` | Recently closed notes: one app-wide `closed.json` (newest 20); the empty ⌘P lists them, ⌘⇧T reopens the newest |
| `src/Switcher.jsx` | ⌘P: keyword hits at once, "Related" (meaning) hits after a 400 ms pause |
| `src/editor/` | Margin's editor, minus attachments, plus `txt://` links and ⌘-click |
| `integrations/alfred/` | The Alfred workflow: `txt-open.sh` (the script) and `build.py` (writes `dist/txt.alfredworkflow`) |

## Notes on disk

- `.md` notes start with front matter: `id`, then `title` and `auto_title: true` when they exist. Other keys are kept as they are.
- **`.txt` notes are never parsed as Markdown.** They open in a plain textarea, so a save writes back only the characters you typed. Their titles live in `~/Library/Application Support/com.cx.txt/index.json`. The test `untitled_txt_keeps_its_uuid_and_bytes` guards this.
- Ids: the front-matter `id`, else the UUID in a TextEdit-era `Untitled-<UUID>.txt` name, else an index entry. Links are `txt://note/<id>`, so renaming a file or changing its title never breaks them.
- A new window creates its file on the first keystroke, so a window opened and closed leaves nothing behind.
- `TXT_NOTES_DIR=<dir>` points a run at another folder, and `TXT_SUPPORT_DIR=<dir>` at another index and config. Test against copies, never the real scratchpad or index.

## Meaning search

- The model is an outside service with Ollama's `/api/embed` API, set in `config.json` in the support folder (default: Ollama, `embeddinggemma`). Nothing about a particular machine belongs in the code.
- **Batched on purpose.** A save calls `Index::touch`; the worker embeds a note only after `idle_secs` (default 180) without edits, on window close (`flush_embeddings`), or in the 5-minute rescan that catches edits made outside txt. Never embed per keystroke or per save: an idle-unloading model would stay awake all day.
- A model that can't be reached leaves notes unmarked, so they retry next batch. Search then shows keyword hits only, and the switcher footer says so.
- Empty notes are recorded with no chunks. Their vector is just the title, which scored 0.63–0.68 against every query and crowded out real matches.
- **Auto titles run in the same batch, before embedding**, because the title is part of what's embedded. Only `derived` notes with ≥40 characters get one; an `auto` title is redone only once the note doubles (`titles` table keeps the length it was titled at). `manual` is never touched, and `Store::set_auto_title` refuses if the file changed since it was read.
- **Title model memory:** `num_ctx: 4096` is load-bearing. Ollama's default context put llama3.2 at 10 GB; at 4k the default `qwen3.5:4b` is 4.1 GB (llama3.2 2.6 GB). The batch ends with a `keep_alive: 0` unload.
- **`think: false` is load-bearing too.** With thinking on, qwen3.5 spends the 24-token budget reasoning and the title comes back empty. A model without thinking accepts the field.
- An auto title rewrites only the front matter. Open windows then update the tab's title without remounting the editor, so the caret and undo history survive (`notes-changed` handler in `App.jsx`).
- The cut-off (`SPREAD` 0.12 below the best match, `FLOOR` 0.60) was calibrated on EmbeddingGemma 2. Another model may need it re-measured.

## Build, test, install

```sh
export PATH=$HOME/.cargo/bin:$PATH
(cd src-tauri && cargo test)
cargo tauri build --bundles app
rm -rf /Applications/txt.app && cp -R src-tauri/target/release/bundle/macos/txt.app /Applications/
LSR=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
$LSR -u src-tauri/target/release/bundle/macos/txt.app; $LSR -f /Applications/txt.app
```

## Releasing

`scripts/release.sh X.Y.Z` builds, signs (Developer ID), notarizes (keychain profile `DiskSight`) and staples into `release/X.Y.Z/`; add `--publish` to tag, upload to GitHub Releases and run `scripts/verify-update-feed.sh`. Bump `package.json`, `package-lock.json`, `tauri.conf.json` and `Cargo.toml` first; it refuses a dirty tree or a used tag.

- **Updates:** `tauri-plugin-updater` reads `releases/latest/download/latest.json` a minute after launch, then daily, and from txt → Check for Updates… (`src/update.rs`). It installs in place and asks before restarting; the restart closes each window first so it saves. Apple Silicon only.
- **The updater key is `~/.tauri/txt.key`** (no password), backed up as secret `TAURI_SIGNING_PRIVATE_KEY_TXT`. Losing it strands every installed copy. `release.sh` refuses to build if `plugins.updater.pubkey` differs from `txt.key.pub`: a wrong pubkey still builds and publishes, and every install silently rejects every update.
- The update tarball is made **after** stapling, by the script, not by Tauri's `createUpdaterArtifacts` (which would pack the unstapled app, and makes every local build demand the key).

## Landmines

- **Never build a window inside a link callback.** `on_open_url` and the single-instance callback run inside Tauri's event dispatch, which holds the lock that `WebviewWindowBuilder::build` needs. On a cold launch from `txt://new`, the app hung with no window. Go through `handle_urls_later`.
- **Files handed to txt arrive through the deep-link plugin, not `RunEvent::Opened`.** The plugin takes over macOS's open callbacks, so `Opened` never fires; it also reports each file twice, hence the 1 s repeat filter in `handle_urls_later`. On a cold launch the URL is only in `deep_link().get_current()`. txt is registered for .txt/.md as **Alternate** (`fileAssociations` rank), so it never becomes the default app.
- **"No window" after a reinstall is usually the Documents prompt.** Every thread that reads the notes folder blocks in `opendir` until it's answered; `sample <pid>` shows it. Don't debug the code until that's ruled out.
- **⌘\ is the editor's "clear formatting"** (as in Margin). Don't bind a menu command to it; the editor wins.
- **Menu commands use `emit_to(label)`, never `emit`.** `emit` reaches every window, so ⌘W closed a tab in every window.
- **File commands are `async`.** Tauri runs plain commands on the main thread, and the first read of `~/Documents` waits for the macOS privacy prompt, which froze every window until it was answered.
- **Two bundles registered for `txt://`.** LaunchServices learns about the build folder's `txt.app` too, and can route links there. Unregister it after every build (the `-u` line above).
- **Live reload is a 1 s mtime poll** (`watch` in `lib.rs`), not FSEvents: ~150 `stat` calls, no reads, no extra crate. It emits `notes-changed` with `by: "disk"`. A window reloads only tabs with nothing pending or in flight, and checks that again after its read; a tab you're typing in keeps your text and gets the conflict banner at its next save.
- **Two processes write `index.json`:** the app and `txt-mcp`. `Store::index()` re-reads the file whenever its mtime isn't the one this process last saw, so neither saves over the other's ids. Always lock through `index()`, never `self.index.lock()` directly. The test `two_processes_never_drop_each_others_index_entries` guards this.
- `txt-mcp resolve <id>` prints `{id,title,path}` or exits 2; Margin's link chips call it. Keep its output stable.
- **`txt-mcp` has no delete or replace tool, on purpose.** `append_note` saves with the `modified` it read, so a simultaneous edit in txt is refused rather than overwritten.
- **`scripts/build-mcp.sh` stages a placeholder first.** `txt-mcp` is in the app's own crate, and tauri-build won't compile that crate until `binaries/txt-mcp-<triple>` exists.
- The installed `/Applications/txt.app/Contents/MacOS/txt-mcp` is a live MCP registration: reinstalling the app replaces it, and deleting the app breaks it.
- Each rebuild is ad-hoc signed with a new signature, so macOS may ask about Documents again after a reinstall.
