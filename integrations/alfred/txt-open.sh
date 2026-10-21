#!/bin/bash
# Alfred `txt` / `text <name>`: open a new note in txt. If txt isn't
# installed, fall back to the old TextEdit scratch file so the keyword
# never stops working.
name="$1"
if [ -d /Applications/txt.app ]; then
  if [ -n "$name" ]; then
    open "txt://new?name=$(/usr/bin/python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1]))' "$name")"
  else
    open "txt://new"
  fi
else
  dir="$HOME/Documents/scratchpad"; mkdir -p "$dir"
  if [ -n "$name" ]; then f="$dir/$name.txt"; else f="$dir/Untitled-$(uuidgen).txt"; fi
  [ -e "$f" ] || : > "$f"
  open -e "$f"
fi
