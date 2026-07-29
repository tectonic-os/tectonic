#!/bin/bash

fetch_verified() {
    local url="$1" sha256="$2" dest="$3"
    curl --retry 3 -fsSLo "$dest" "$url"
    echo "${sha256}  ${dest}" | sha256sum -c -
}

fetch_extract() {
    local url="$1" sha256="$2" dir="$3" archive
    shift 3
    archive="/tmp/fetch.$$.${url##*/}"
    fetch_verified "$url" "$sha256" "$archive"
    mkdir -p "$dir"
    case "$archive" in
        *.zip | *.plasmoid) unzip -q "$archive" -d "$dir" "$@" ;;
        *.tar.zst) tar --use-compress-program=zstd -xf "$archive" -C "$dir" "$@" ;;
        *) tar -xf "$archive" -C "$dir" "$@" ;;
    esac
    rm -f "$archive"
}

fetch_install_bin() {
    local url="$1" sha256="$2" name="$3" inner="${4:-$3}" tmp
    tmp="/tmp/fetch-bin.$$.${name}"
    case "$url" in
        *.zip | *.tar.gz | *.tgz | *.tar.xz | *.tar.zst)
            fetch_extract "$url" "$sha256" "$tmp"
            install -m755 "${tmp}/${inner}" "/usr/bin/${name}"
            rm -rf "$tmp"
            ;;
        *)
            fetch_verified "$url" "$sha256" "$tmp"
            install -m755 "$tmp" "/usr/bin/${name}"
            rm -f "$tmp"
            ;;
    esac
}

fetch_install_rpm() {
    local url="$1" sha256="$2" rpm
    rpm="/tmp/fetch.$$.${url##*/}"
    fetch_verified "$url" "$sha256" "$rpm"
    dnf5 install -y "$rpm"
    rm -f "$rpm"
}
