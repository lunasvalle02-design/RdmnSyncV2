# Compila los binarios de release para Windows y Linux dentro de Docker y los deja en dist\.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
docker build -q -t rdmnsync-builder -f "$root\docker\Dockerfile" "$root\docker" | Out-Null
New-Item -ItemType Directory -Force "$root\dist" | Out-Null
docker run --rm `
  -v "${root}:/src" `
  -v rdmnsync-cargo-registry:/usr/local/cargo/registry `
  -v rdmnsync-target:/target `
  -v "${root}\dist:/dist" `
  rdmnsync-builder sh -c @'
set -e
cargo test --release --target x86_64-unknown-linux-gnu
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-unknown-linux-gnu
cp /target/x86_64-pc-windows-gnu/release/rdmnsync.exe /dist/rdmnsync-windows-x64.exe
cp /target/x86_64-unknown-linux-gnu/release/rdmnsync /dist/rdmnsync-linux-x64
ls -lh /dist
'@
