#!/usr/bin/env bash
# Checks the published update feed the way an installed txt reads it: the feed
# is reachable, names VERSION (default: whatever it names), its download is
# there, and the download's signature verifies against the app's pubkey.
#
#   scripts/verify-update-feed.sh [VERSION]
set -euo pipefail
cd "$(dirname "$0")/.."
FEED="https://github.com/cxrobx/txt/releases/latest/download/latest.json"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$FEED" -o "$TMP/latest.json" || { echo "FAIL: no feed at $FEED" >&2; exit 1; }
read -r VER URL < <(node -e 'const f=require(process.argv[1]); const p=f.platforms["darwin-aarch64"]; console.log(f.version, p.url)' "$TMP/latest.json")
[ -z "${1:-}" ] || [ "$VER" = "$1" ] || { echo "FAIL: the feed names $VER, not $1" >&2; exit 1; }
node -e 'require("fs").writeFileSync(process.argv[2], require(process.argv[1]).platforms["darwin-aarch64"].signature)' "$TMP/latest.json" "$TMP/sig"
curl -fsSL "$URL" -o "$TMP/txt.app.tar.gz" || { echo "FAIL: download missing: $URL" >&2; exit 1; }
node scripts/verify-updater-signature.mjs "$TMP/sig" "$TMP/txt.app.tar.gz" src-tauri/tauri.conf.json | tail -1
echo "feed OK: txt $VER"
