#!/usr/bin/env bash
# Build, sign, notarize and staple a txt release into release/VERSION/.
#
#   scripts/release.sh VERSION [--notes-file FILE] [--allow-dirty] [--publish]
#
# Without --publish this only builds: nothing is tagged, pushed or uploaded.
# With --publish it tags HEAD, creates the GitHub release as a draft, uploads,
# makes it public, and checks the update feed.
#
# Output: txt-VERSION.dmg (to install), txt.app.tar.gz + .sig (what the
# updater downloads), latest.json (the update feed), txt.alfredworkflow, and a
# .sha256 beside each download. The unpacked .app is deleted afterwards: every
# .app left on disk shows up in Spotlight and Alfred as another "txt".
#
# Needs: the Developer ID certificate in the login keychain, the notarytool
# keychain profile DiskSight (same team), and the updater key: ~/.tauri/txt.key,
# or TAURI_SIGNING_PRIVATE_KEY via `secret run -k TAURI_SIGNING_PRIVATE_KEY_TXT`.
# Override with CSC_NAME and APPLE_KEYCHAIN_PROFILE. No credential is printed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="cxrobx/txt"
CSC_NAME="${CSC_NAME:-Christopher Robinson (CCYV5HQZCM)}"
APPLE_KEYCHAIN_PROFILE="${APPLE_KEYCHAIN_PROFILE:-DiskSight}"
IDENTITY="Developer ID Application: $CSC_NAME"
KEY_FILE="$HOME/.tauri/txt.key"

VERSION=""; PUBLISH=0; ALLOW_DIRTY=0; NOTES_FILE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --publish) PUBLISH=1 ;;
    --allow-dirty) ALLOW_DIRTY=1 ;;
    --notes-file) shift; NOTES_FILE="${1:-}" ;;
    -h|--help) sed -n '2,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "Unknown option: $1" >&2; exit 2 ;;
    *) if [ -z "$VERSION" ]; then VERSION="$1"; else echo "Unexpected argument: $1" >&2; exit 2; fi ;;
  esac
  shift
done

step() { printf '\n==> %s\n' "$*"; }
die() { printf 'release: %s\n' "$*" >&2; exit 1; }

[ -n "$VERSION" ] || die "usage: scripts/release.sh VERSION [--notes-file FILE] [--allow-dirty] [--publish]"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "VERSION must be x.y.z (got '$VERSION')"
[ "$(uname)" = Darwin ] || die "signing and notarizing need macOS"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

step "Checking the tree, tag and version"
if [ -n "$(git status --porcelain)" ]; then
  if [ "$ALLOW_DIRTY" = 1 ] && [ "$PUBLISH" = 0 ]; then
    echo "WARNING: building from a dirty tree (--allow-dirty). Dry runs only; do not ship it."
  else
    git status --short >&2
    die "the git tree is dirty. Commit first so the release matches a commit (--allow-dirty is refused with --publish)"
  fi
fi
TAG="v$VERSION"
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && die "tag $TAG already exists locally"
REMOTE_TAG=0; git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1 || REMOTE_TAG=$?
case "$REMOTE_TAG" in
  0) die "tag $TAG already exists on origin" ;;
  2) ;;
  *) echo "WARNING: could not reach origin to check for $TAG; only the local tags were checked." ;;
esac
PKG="$(node -p "require('./package.json').version")"
CONF="$(node -p "require('./src-tauri/tauri.conf.json').version")"
CARGO="$(sed -n 's/^version = "\([^"]*\)"/\1/p' src-tauri/Cargo.toml | head -1)"
[ "$PKG $CONF $CARGO" = "$VERSION $VERSION $VERSION" ] \
  || die "versions are package.json=$PKG tauri.conf.json=$CONF Cargo.toml=$CARGO; bump all three to $VERSION first"
if [ "$PUBLISH" = 1 ] && [ -z "$(git branch -r --contains HEAD 2>/dev/null)" ]; then die "HEAD is not on origin yet; push it first so the tag lands on this commit"; fi

step "Checking the updater key"
# A pubkey in tauri.conf.json that doesn't match the signing key still builds,
# signs and publishes; every installed copy then rejects every update.
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  [ -f "$KEY_FILE" ] || die "no updater key: $KEY_FILE is missing and TAURI_SIGNING_PRIVATE_KEY is unset (secret run -k TAURI_SIGNING_PRIVATE_KEY_TXT)"
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY_FILE")"
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
if [ -f "$KEY_FILE.pub" ]; then
  node -e "process.exit(require('./src-tauri/tauri.conf.json').plugins.updater.pubkey === require('fs').readFileSync(process.argv[1],'utf8').trim() ? 0 : 1)" "$KEY_FILE.pub" \
    || die "plugins.updater.pubkey in tauri.conf.json does not match $KEY_FILE.pub; updates would be rejected by every install"
  echo "pubkey matches $KEY_FILE.pub"
fi

step "Checking signing and notarization credentials"
[[ "$(security find-identity -v -p codesigning)" == *"\"$IDENTITY\""* ]] || die "no '$IDENTITY' certificate in the keychain"
xcrun notarytool history --keychain-profile "$APPLE_KEYCHAIN_PROFILE" >/dev/null 2>&1 \
  || die "the notarytool keychain profile '$APPLE_KEYCHAIN_PROFILE' does not work. Not creating credentials; fix or set APPLE_KEYCHAIN_PROFILE"
echo "identity: $IDENTITY"; echo "notary profile: $APPLE_KEYCHAIN_PROFILE (works)"

step "Testing"
(cd src-tauri && cargo test --quiet 2>&1 | grep -E "^test result|FAILED|panicked" ) || die "cargo test failed"

step "Building and signing txt $VERSION"
APPLE_SIGNING_IDENTITY="$IDENTITY" cargo tauri build --bundles app
BUILT="src-tauri/target/release/bundle/macos/txt.app"
# The build folder's copy would also claim txt:// links; see CLAUDE.md.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$BUILT" 2>/dev/null || true
OUT="release/$VERSION"; rm -rf "$OUT"; mkdir -p "$OUT"
APP="$OUT/txt.app"; ditto "$BUILT" "$APP"
codesign --verify --deep --strict "$APP" || die "the app's signature does not verify"
for bin in txt txt-mcp; do
  [[ "$(codesign -dv "$APP/Contents/MacOS/$bin" 2>&1)" == *"(runtime)"* ]] || die "$bin is not signed with the hardened runtime"
done

notarize() { # path to a .zip or .dmg
  xcrun notarytool submit "$1" --keychain-profile "$APPLE_KEYCHAIN_PROFILE" --wait --output-format json > "$OUT/notary.json" \
    || { cat "$OUT/notary.json" >&2; die "notarytool failed for $1"; }
  node -e "const r=require(process.argv[1]); if (r.status !== 'Accepted') { console.error(r); process.exit(1) }" "$ROOT/$OUT/notary.json" \
    || die "Apple did not accept $1; see: xcrun notarytool log <id> --keychain-profile $APPLE_KEYCHAIN_PROFILE"
}

step "Notarizing the app"
ditto -c -k --keepParent "$APP" "$OUT/notarize.zip"
notarize "$OUT/notarize.zip"; rm "$OUT/notarize.zip"
xcrun stapler staple "$APP"
[[ "$(spctl -a -t exec -vv "$APP" 2>&1)" == *"source=Notarized Developer ID"* ]] || die "Gatekeeper does not accept the stapled app"

step "Packaging the update"
COPYFILE_DISABLE=1 tar -czf "$OUT/txt.app.tar.gz" -C "$OUT" txt.app
cargo tauri signer sign "$OUT/txt.app.tar.gz" >/dev/null
[ -s "$OUT/txt.app.tar.gz.sig" ] || die "the updater signature was not written"
node scripts/verify-updater-signature.mjs "$OUT/txt.app.tar.gz.sig" "$OUT/txt.app.tar.gz" src-tauri/tauri.conf.json | tail -1

step "Making the disk image"
STAGE="$OUT/dmg"; mkdir -p "$STAGE"; ditto "$APP" "$STAGE/txt.app"; ln -s /Applications "$STAGE/Applications"
DMG="$OUT/txt-$VERSION.dmg"
hdiutil create -quiet -volname "txt $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO "$DMG"
rm -rf "$STAGE"
codesign --sign "$IDENTITY" --timestamp "$DMG"
notarize "$DMG"; rm "$OUT/notary.json"
xcrun stapler staple "$DMG"
rm -rf "$APP"

step "Writing the update feed and the Alfred workflow"
NOTES="${NOTES_FILE:+$(cat "$NOTES_FILE")}"
[ -n "$NOTES" ] || NOTES="txt $VERSION"
node -e '
const [version, notes, sigPath, out, repo] = process.argv.slice(1);
const feed = { version, notes, pub_date: new Date().toISOString().replace(/\.\d+Z$/, "Z"),
  platforms: { "darwin-aarch64": { signature: require("fs").readFileSync(sigPath, "utf8").trim(),
    url: `https://github.com/${repo}/releases/download/v${version}/txt.app.tar.gz` } } };
require("fs").writeFileSync(out, JSON.stringify(feed, null, 2) + "\n");
' "$VERSION" "$NOTES" "$OUT/txt.app.tar.gz.sig" "$OUT/latest.json" "$REPO"
python3 integrations/alfred/build.py >/dev/null && cp dist/txt.alfredworkflow "$OUT/"
(cd "$OUT" && for f in "txt-$VERSION.dmg" txt.app.tar.gz txt.alfredworkflow; do shasum -a 256 "$f" > "$f.sha256"; done)
ls -la "$OUT"

FILES=("$OUT/txt-$VERSION.dmg" "$OUT/txt.app.tar.gz" "$OUT/txt.app.tar.gz.sig" "$OUT/latest.json" "$OUT/txt.alfredworkflow" "$OUT"/*.sha256)
NOTES_ARG=(--notes "$NOTES"); [ -n "$NOTES_FILE" ] && NOTES_ARG=(--notes-file "$NOTES_FILE")
if [ "$PUBLISH" = 0 ]; then
  echo; echo "Built, not published. To publish, rerun with --publish."
  exit 0
fi

step "Publishing $TAG"
git tag -a "$TAG" -m "txt $VERSION" && git push origin "$TAG"
gh release create "$TAG" --repo "$REPO" --draft --title "txt $VERSION" "${NOTES_ARG[@]}" "${FILES[@]}"
gh release edit "$TAG" --repo "$REPO" --draft=false --latest
scripts/verify-update-feed.sh "$VERSION"
