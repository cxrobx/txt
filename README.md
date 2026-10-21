# txt

Quick notes, one keystroke away. Type `txt` in Alfred and a small note window opens, ready to type. Each window has tabs, and ⌘P switches any tab to any note. Notes are plain files in a folder, so any other app can read them.

Source available under the [Business Source License 1.1](LICENSE): free for personal use and inside your own organisation. It converts to Apache 2.0 on 2030-10-21.

## What it does

- **New window, instantly.** `txt` opens a new note; `txt groceries` opens one titled "groceries". The app stays running with no windows open, so the next one appears at once. A window you close without typing leaves no file behind.
- **Tabs and a switcher.** ⌘T opens a new tab, ⌘P finds a note by title or text. In the switcher, ↩ opens a note in this tab, ⌘↩ in a new tab, ⌥↩ in a new window. ⌘1–9 jump between tabs, and ⌘W closes one.
- **Markdown that renders as you type.** Headings, lists, checklists, quotes, code, tables and links. Select text for a formatting toolbar; ⌘K adds a link, and ⌘-click follows one. The editor comes from [Margin Notes](https://github.com/cxrobx/Margin).
- **Plain files.** Notes live in `~/Documents/scratchpad` as `.md` files with a small front-matter block (`id`, `title`). Older `.txt` files open as plain text and are saved back exactly as typed, never converted to Markdown.
- **Search by meaning (optional).** With a local embedding model running, ⌘P also lists notes *related* to what you typed, not just ones containing the words. See [Meaning search](#meaning-search).
- **Titles that write themselves (optional).** A note you haven't titled gets a short title from a small local model, written once the note has some text and refreshed only if it roughly doubles. Your own titles are never replaced, and nothing leaves your Mac.
- **Live reload.** An edit made outside txt (an agent through the MCP, another editor, a sync) shows up in open tabs within a second. A tab you're typing in keeps your text and asks before loading theirs.
- **Open With.** Finder's Open With, or any app that hands txt a file in the notes folder, opens it as that note. txt never makes itself the default app for .txt or .md.
- **Links that don't break.** `txt://note/<id>` opens a note (File → Copy Link to Note, ⇧⌘C). Renaming a note or its file doesn't break the link. `txt://new?name=…` opens a new named note.

## Install

Build from source (macOS 13+, Rust, Node 20+):

```sh
npm install
npx tauri build --bundles app
cp -R src-tauri/target/release/bundle/macos/txt.app /Applications/
```

The first time txt opens, macOS asks whether it may use your Documents folder; that's where the notes are kept.

### Alfred

```sh
python3 integrations/alfred/build.py   # writes dist/txt.alfredworkflow
open dist/txt.alfredworkflow
```

The workflow adds one keyword: `txt [name]`. If txt isn't installed, it creates a TextEdit file in the same folder instead.

## Meaning search

Keyword search always works. To also find notes by meaning, run a local embedding model behind Ollama's API:

```sh
ollama pull embeddinggemma
```

txt then embeds your notes in the background, in batches: a save only marks a note as changed, and the changes are embedded together once you've stopped typing for a few minutes, or when you close the window. Models served this way unload when idle, so the model is only in memory while a batch or a search runs. The index (a few MB) lives in `~/Library/Application Support/com.cx.txt/index.db`.

To use a different model or server, create `~/Library/Application Support/com.cx.txt/config.json`; every key is optional:

```json
{ "url": "http://127.0.0.1:11434", "model": "embeddinggemma", "idle_secs": 180 }
```

`model` can be any embedding model the server offers; EmbeddingGemma and `nomic-embed-text` get their recommended task prefixes. Changing it re-embeds every note.

## Auto titles

With Ollama running and `ollama pull qwen3.5:4b`, txt titles untitled notes in the same background batches as meaning search. The model is unloaded as soon as each batch finishes. A `.txt` note's title is stored in txt's index, so its file is never rewritten; a `.md` note's goes in its front matter as `title:` with `auto_title: true`. Type a title yourself to replace it for good.

Settings go under `titles` in the same `config.json`:

```json
{ "titles": { "enabled": true, "url": "http://127.0.0.1:11434", "model": "qwen3.5:4b" } }
```

## MCP

`txt.app` ships an MCP server, so an agent can find and add to your notes. It has six tools: `search_notes` (words, plus meaning when the embedding model runs), `list_notes`, `read_note`, `create_note`, `append_note` and `note_link`. Nothing it does deletes or replaces text. `txt-mcp resolve <id>` prints a note's title and path as JSON (exit 2 if there's no such note), for apps that show txt links. For Claude Code:

```sh
claude mcp add --scope user txt /Applications/txt.app/Contents/MacOS/txt-mcp
```

## Develop

```sh
(cd src-tauri && cargo test)                     # the note store
TXT_NOTES_DIR=/tmp/txt-notes TXT_SUPPORT_DIR=/tmp/txt-support npx tauri dev   # a scratch folder and index
```
