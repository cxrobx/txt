#!/usr/bin/env python3
"""Builds txt.alfredworkflow: one keyword, `txt [name]`, that opens a new note
in txt (or, if txt isn't installed, a TextEdit scratch file)."""
import plistlib, shutil, sys, tempfile, zipfile
from pathlib import Path

here = Path(__file__).parent
root = here.parent.parent
out = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "dist" / "txt.alfredworkflow"
keyword, action = "6B1F7C2E-2D4A-4F0B-9E57-0C3A1D9B8E10", "A7C1E0B2-7F3D-4E0A-9C61-5B2D8E4F1A01"

info = {
    "bundleid": "com.cx.txt.alfred",
    "name": "txt",
    "description": "Open a new txt note",
    "createdby": "CX Ventures",
    "webaddress": "https://github.com/cxrobx/txt",
    "readme": "txt — open a new note.\ntxt groceries — open a new note titled “groceries”.\n\nNeeds txt.app in /Applications; without it, falls back to a TextEdit file in ~/Documents/scratchpad.",
    "version": "0.1.0",
    "disabled": False,
    "objects": [
        {"uid": keyword, "type": "alfred.workflow.input.keyword", "version": 1, "config": {
            "keyword": "txt", "text": "New txt note", "subtext": "Type a name to title it, or just press ↩",
            "argumenttype": 1, "withspace": True}},
        {"uid": action, "type": "alfred.workflow.action.script", "version": 2, "config": {
            "type": 0, "script": (here / "txt-open.sh").read_text(), "scriptargtype": 1,
            "scriptfile": "", "escaping": 0, "concurrently": False}},
    ],
    "connections": {keyword: [{"destinationuid": action, "modifiers": 0, "modifiersubtext": "", "vitoclose": False}]},
    "uidata": {keyword: {"xpos": 40, "ypos": 40}, action: {"xpos": 260, "ypos": 40}},
}

with tempfile.TemporaryDirectory() as tmp:
    plistlib.dump(info, open(Path(tmp) / "info.plist", "wb"))
    shutil.copy(root / "src-tauri" / "icons" / "128x128@2x.png", Path(tmp) / "icon.png")
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for name in ("info.plist", "icon.png"):
            z.write(Path(tmp) / name, name)
print(out)
