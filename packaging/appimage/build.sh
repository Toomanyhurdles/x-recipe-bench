#!/bin/sh
# Builds the release binary and packs it into an AppImage in dist/.
# Needs: cargo, and appimagetool (path in $APPIMAGETOOL, or on PATH).
set -eu
cd "$(dirname "$0")/../.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TOOL=${APPIMAGETOOL:-appimagetool}

cargo build --release -p x-recipe-bench

APPDIR=target/appimage/AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
cp target/release/x-recipe-bench "$APPDIR/usr/bin/"
mkdir -p "$APPDIR/usr/share/doc/x-recipe-bench"
cp LICENSE licenses/*.txt "$APPDIR/usr/share/doc/x-recipe-bench/"
cp packaging/appimage/x-recipe-bench.desktop packaging/appimage/x-recipe-bench.png "$APPDIR/"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/x-recipe-bench" "$@"
RUN
chmod +x "$APPDIR/AppRun"

mkdir -p dist
OUT="dist/X-Recipe-Bench-$VERSION-x86_64.AppImage"
rm -f "$OUT"
ARCH=x86_64 "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT"
echo "Built $OUT"
