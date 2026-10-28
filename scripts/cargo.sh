#!/usr/bin/env sh
# Ejecuta cargo dentro del contenedor de compilación.
# Uso: scripts/cargo.sh check --target x86_64-pc-windows-gnu
# RDMNSYNC_TARGET_VOLUME permite usar otro volumen de compilación (p. ej. uno por worktree).
set -e
cd "$(dirname "$0")/.."
ROOT="$(pwd -W 2>/dev/null || pwd)"
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "$ROOT:/src" \
  -v rdmnsync-cargo-registry:/usr/local/cargo/registry \
  -v "${RDMNSYNC_TARGET_VOLUME:-rdmnsync-target}:/target" \
  ${DOCKER_EXTRA:-} \
  rdmnsync-builder cargo "$@"
