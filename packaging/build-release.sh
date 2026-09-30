#!/usr/bin/env bash
# Build release artifacts into dist/: a static x86_64 binary tarball, .deb, .rpm and an
# Arch package, plus SHA256SUMS. Needs: rustup target x86_64-unknown-linux-musl, clang,
# cargo-deb, cargo-generate-rpm, and makepkg (for the Arch package).
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TARGET=x86_64-unknown-linux-musl
NAME="folderblog-${VERSION}-x86_64-linux"
rm -rf dist && mkdir -p dist

echo "== tests"
FOLDERBLOG_NO_NOTIFY=1 cargo test -q

echo "== static binary ($TARGET)"
# ring (TLS) has C code; clang can target musl without a musl cross toolchain.
CC_x86_64_unknown_linux_musl=clang cargo build --release --locked --target "$TARGET"
BIN="target/$TARGET/release/folderblog"
file "$BIN" | grep -q "static" || { echo "binary is not static"; exit 1; }

echo "== tarball"
mkdir -p "dist/$NAME"
cp "$BIN" packaging/folderblog.service README.md LICENSE "dist/$NAME/"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"

echo "== deb"
cargo deb --no-build --no-strip --target "$TARGET" -o "dist/folderblog_${VERSION}-1_amd64.deb"

echo "== rpm"
cargo generate-rpm --target "$TARGET" -o "dist/folderblog-${VERSION}-1.x86_64.rpm"

echo "== arch"
ARCHDIR=$(mktemp -d)
cp packaging/arch/PKGBUILD "$ARCHDIR/"
cp "dist/$NAME.tar.gz" "$ARCHDIR/"
# Build from the local tarball instead of downloading the release.
sed -i "s|^source=.*|source=(\"$NAME.tar.gz\")|; s|^pkgver=.*|pkgver=$VERSION|" "$ARCHDIR/PKGBUILD"
(cd "$ARCHDIR" && PKGDEST="$PWD" makepkg -f --nodeps --noconfirm >/dev/null)
cp "$ARCHDIR"/folderblog-bin-*.pkg.tar.zst dist/
rm -rf "$ARCHDIR" "dist/$NAME"

(cd dist && sha256sum -- * > SHA256SUMS)
echo "== done"
ls -la dist
