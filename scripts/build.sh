#!/usr/bin/env sh
# Compila los binarios de release para Windows y Linux dentro de Docker y los deja en dist/.
set -e
cd "$(dirname "$0")/.."
ROOT="$(pwd -W 2>/dev/null || pwd)"
export MSYS_NO_PATHCONV=1
docker build -q -t rdmnsync-builder -f docker/Dockerfile docker >/dev/null
mkdir -p dist
docker run --rm \
  -v "$ROOT:/src" \
  -v rdmnsync-cargo-registry:/usr/local/cargo/registry \
  -v "${RDMNSYNC_TARGET_VOLUME:-rdmnsync-target}:/target" \
  -v "$ROOT/dist:/dist" \
  rdmnsync-builder sh -c '
    set -e
    cargo test --release --target x86_64-unknown-linux-gnu
    cargo build --release --target x86_64-pc-windows-gnu
    cargo build --release --target x86_64-unknown-linux-gnu
    cp /target/x86_64-pc-windows-gnu/release/rdmnsync.exe /dist/rdmnsync-windows-x64.exe
    cp /target/x86_64-unknown-linux-gnu/release/rdmnsync /dist/rdmnsync-linux-x64
    ls -lh /dist'
