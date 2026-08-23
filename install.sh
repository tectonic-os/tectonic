#!/bin/sh
set -eu

repo="tectonic-os/tectonic"

die() {
    printf "install: %s\n" "$*" >&2
    exit 1
}

command -v curl > /dev/null || die "curl is required"
command -v tar > /dev/null || die "tar is required"
command -v sha256sum > /dev/null || die "sha256sum is required"

case "$(uname -m)" in
    x86_64) ;;
    *) die "only x86_64 Linux is published" ;;
esac

version="${1:-}"
if [ -n "$version" ]; then
    version="${version#v}"
else
    # The redirect names the tag, so latest costs no rate-limited API call.
    url="$(curl -fsSL -I -o /dev/null -w '%{url_effective}' \
        "https://github.com/$repo/releases/latest")" \
        || die "cannot resolve the latest release"
    version="${url##*/tag/v}"
    if [ -z "$version" ] || [ "$version" = "$url" ]; then
        die "cannot read a version out of $url"
    fi
fi

# The pairs init::assets() looks in: a binary without its assets beside it
# scaffolds from whatever stale copy the host already has.
if [ "$(id -u)" -eq 0 ]; then
    bindir="/usr/local/bin"
    assetsdir="/usr/local/share/tectonic/assets"
else
    [ -n "${HOME:-}" ] || die "HOME is not set"
    bindir="$HOME/.local/bin"
    assetsdir="${XDG_DATA_HOME:-$HOME/.local/share}/tectonic/assets"
fi

if [ -e "$bindir/assets" ]; then
    die "$bindir/assets outranks $assetsdir; remove it first"
fi

if [ -n "${TECT_ASSETS:-}" ]; then
    printf "install: warning: TECT_ASSETS=%s outranks %s\n" \
        "$TECT_ASSETS" "$assetsdir" >&2
fi

parent="${assetsdir%/*}"
mkdir -p "$bindir" || die "cannot create $bindir"
mkdir -p "$parent" || die "cannot create $parent"

if [ ! -w "$bindir" ] || [ ! -w "$parent" ]; then
    if [ "$(id -u)" -eq 0 ]; then
        die "cannot write $bindir or $parent"
    fi
    die "cannot write $bindir or $parent; run as root to install to /usr/local"
fi

# Beside the destination, so swapping the assets in is a rename.
tmp="$(mktemp -d "$parent/.install.XXXXXXXX")" || die "cannot create a temp dir"
trap 'rm -rf "$tmp"' EXIT

asset="tect-v${version}-x86_64-linux-musl.tar.gz"
url="https://github.com/$repo/releases/download/v${version}/${asset}"

curl -fsSL --retry 3 -o "$tmp/$asset" "$url" \
    || die "cannot fetch $url"
curl -fsSL --retry 3 -o "$tmp/sha256" "${url}.sha256" \
    || die "cannot fetch ${url}.sha256"
(cd "$tmp" && printf '%s  %s\n' "$(cat sha256)" "$asset" \
    | sha256sum --check --status) \
    || die "$asset does not match its published checksum"

tar -xzf "$tmp/$asset" -C "$tmp" || die "cannot extract $asset"
[ -f "$tmp/tect" ] || die "$asset holds no tect"
[ -d "$tmp/assets" ] || die "$asset holds no assets directory"

case "$assetsdir" in
    */tectonic/assets) ;;
    *) die "refusing to remove $assetsdir" ;;
esac

# Swapped rather than merged: an asset a release dropped must not survive.
rm -rf "$assetsdir"
mv "$tmp/assets" "$assetsdir" || die "cannot place $assetsdir"
mv -f "$tmp/tect" "$bindir/tect" || die "cannot place $bindir/tect"
chmod 755 "$bindir/tect"

printf "install: tect %s is at %s\n" "$version" "$bindir/tect"
printf "install: its assets are at %s\n" "$assetsdir"

case ":$PATH:" in
    *":$bindir:"*) ;;
    *) printf "install: %s is not on your PATH\n" "$bindir" >&2 ;;
esac
