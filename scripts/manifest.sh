#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

crate=tools/manifest
bin="${crate}/target/release/manifest"

die() {
    echo "manifest: $*" >&2
    exit 1
}

stale() {
    [ -x "$bin" ] || return 0
    [ -n "$(find "${crate}/src" "${crate}/Cargo.toml" "${crate}/Cargo.lock" \
        -newer "$bin" -print -quit 2> /dev/null)" ]
}

if stale; then
    command -v cargo > /dev/null 2>&1 || die "$(
        cat <<'EOF'
cargo not found, and the manifest parser has to be built before anything
can read modules.kdl.

  rustup:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  brew:    brew install rust
  fedora:  sudo dnf install cargo   (not on an image-based system)

CI needs no setup: the runner image ships a Rust toolchain.
EOF
    )"
    cargo build --release --locked --quiet --manifest-path "${crate}/Cargo.toml" >&2
fi

exec "$bin" "$@"
