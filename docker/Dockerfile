# Imagen de compilación: Linux nativo + Windows (x86_64-pc-windows-gnu) con mingw-w64.
FROM rust:1.98-bookworm

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      mingw-w64 pkg-config cmake \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-pc-windows-gnu x86_64-unknown-linux-gnu

ENV CARGO_TARGET_DIR=/target \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc \
    AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar \
    WINDRES_x86_64_pc_windows_gnu=x86_64-w64-mingw32-windres

WORKDIR /src
