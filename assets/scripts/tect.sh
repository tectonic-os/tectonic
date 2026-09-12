#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
    echo "tect: $*" >&2
    exit 1
}

REPO="tectonic-os/tectonic"

# One declared value out of the manifest, and nothing derived from it. A
# repository that declares no `tect-version` is not pinned and takes the latest
# release, resolved through the redirect `releases/latest` answers with. The API
# rate-limits unauthenticated callers.
line="$(sed -n '/^[[:space:]]*tect-version[[:space:]]/p' repo.kdl)"
if [[ "$line" =~ ^[[:space:]]*tect-version[[:space:]]+\"([^\"]+)\"(.*)$ ]]; then
    version="${BASH_REMATCH[1]}"
    rest="${BASH_REMATCH[2]%%//*}"
elif [ -n "$line" ]; then
    die "repo.kdl declares no readable tect-version"
else
    latest="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
        "https://github.com/${REPO}/releases/latest")" \
        || die "cannot resolve the latest release of ${REPO}"
    [[ "$latest" =~ /tag/v([^/]+)$ ]] \
        || die "cannot read a release out of ${latest}"
    version="${BASH_REMATCH[1]}"
    rest=""
    echo "tect: repo.kdl pins no release, using the latest, ${version}" >&2
fi

sha256=""
if [[ "$rest" =~ (^|[[:space:]])sha256[[:space:]]*= ]]; then
    if [[ "$rest" =~ (^|[[:space:]])sha256[[:space:]]*=[[:space:]]*\"([^\"]*)\" ]]; then
        sha256="${BASH_REMATCH[2]}"
    else
        die "repo.kdl declares no readable sha256 for tect-version"
    fi
    [[ "$sha256" =~ ^[0-9a-f]{64}$ ]] \
        || die "repo.kdl declares a malformed sha256 for tect-version"
fi

case "$(uname -m)" in
    x86_64 | aarch64) arch="$(uname -m)" ;;
    *) die "Linux is published for x86_64 and aarch64, and this is $(uname -m)" ;;
esac

# One pin, one architecture: the sha256 repo.kdl declares is the x86_64
# asset's, so it cannot verify another architecture's download.
if [ -n "$sha256" ] && [ "$arch" != "x86_64" ]; then
    die "repo.kdl pins the sha256 of the x86_64 release, and this is ${arch}"
fi

bin="${TECT_BIN:-out/tect-${version}${sha256:+-${sha256}}}"

if [ ! -x "$bin" ]; then
    asset="tect-v${version}-${arch}-linux-musl.tar.gz"
    url="https://github.com/${REPO}/releases/download/v${version}/${asset}"

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

# Where the binary is: the build mounts it.
if [ "${1:-}" = "--path" ]; then
    echo "$bin"
    exit 0
fi

exec "$bin" "$@"
