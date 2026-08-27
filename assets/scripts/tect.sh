#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
    echo "tect: $*" >&2
    exit 1
}

# One declared value out of the manifest, and nothing derived from it.
version="$(sed -n 's/^tect-version "\([^"]*\)".*$/\1/p' repo.kdl)"
[ -n "$version" ] || die "repo.kdl declares no tect-version"
sha256="$(sed -n 's/^tect-version "[^"]*" sha256="\([^"]*\)"$/\1/p' repo.kdl)"

bin="${TECT_BIN:-out/tect-${version}}"

if [ ! -x "$bin" ]; then
    asset="tect-v${version}-x86_64-linux-musl.tar.gz"
    url="https://github.com/tectonic-os/tectonic/releases/download/v${version}/${asset}"

    mkdir -p out
    tmp="$(mktemp -d -p out download.XXXXXXXX)"
    trap 'rm -rf "$tmp"' EXIT

    curl -fsSL --retry 3 --retry-all-errors -o "${tmp}/${asset}" "$url" \
        || die "cannot fetch ${url}"
    # A declared sha256 is the verifier; the checksum fetched beside the
    # tarball is the fallback for a repository declaring none, and proves
    # the download and nothing more.
    if [ -n "$sha256" ]; then
        (cd "$tmp" && printf '%s  %s\n' "$sha256" "$asset" \
            | sha256sum --check --status) \
            || die "${asset} does not match the sha256 repo.kdl declares"
    else
        curl -fsSL --retry 3 --retry-all-errors -o "${tmp}/sha256" "${url}.sha256" \
            || die "cannot fetch ${url}.sha256"
        (cd "$tmp" && printf '%s  %s\n' "$(cat sha256)" "$asset" \
            | sha256sum --check --status) \
            || die "${asset} does not match its published checksum"
    fi

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
