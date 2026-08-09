#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# renovate: datasource=github-releases depName=tectonic-os/tectonic
version="0.0.0"

die() {
    echo "tect: $*" >&2
    exit 1
}

bin="${TECT_BIN:-out/tect-${version}}"

if [ ! -x "$bin" ]; then
    asset="tect-v${version}-x86_64-linux-musl.tar.gz"
    url="https://github.com/tectonic-os/tectonic/releases/download/v${version}/${asset}"

    mkdir -p out
    tmp="$(mktemp -d -p out download.XXXXXXXX)"
    trap 'rm -rf "$tmp"' EXIT

    curl -fsSL --retry 3 -o "${tmp}/${asset}" "$url" \
        || die "cannot fetch ${url}"
    curl -fsSL --retry 3 -o "${tmp}/sha256" "${url}.sha256" \
        || die "cannot fetch ${url}.sha256"
    (cd "$tmp" && printf '%s  %s\n' "$(cat sha256)" "$asset" \
        | sha256sum --check --status) \
        || die "${asset} does not match its published checksum"

    tar -xzf "${tmp}/${asset}" -C "$tmp" tect
    mv "${tmp}/tect" "$bin"
    rm -rf "$tmp"
    trap - EXIT
fi

# Where the binary is, rather than running it: the build mounts it.
if [ "${1:-}" = "--path" ]; then
    echo "$bin"
    exit 0
fi

exec "$bin" "$@"
