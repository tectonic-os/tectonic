#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

remote_root="modules/.remote"
stamp_root="build/remote-modules"

die() {
    echo "fetch-modules: $*" >&2
    exit 1
}

remotes="$(./scripts/tect.sh plan --json | jq -r '
    .remotes[]
    | [.name, .dir, .ref, .sha256, .url, (.path // ""), .file]
    | join("|")')"
pins=()
[ -z "$remotes" ] || mapfile -t pins <<< "$remotes"

if [ -d "$remote_root" ]; then
    pinned="$(cut -d'|' -f1 <<< "$remotes")"
    for dir in "$remote_root"/*/; do
        [ -d "$dir" ] || continue
        name="$(basename "$dir")"
        if ! grep -qxF "$name" <<< "$pinned"; then
            echo "fetch-modules: ${name} is no longer pinned, removing"
            rm -rf "$dir" "${stamp_root}/${name}.pin"
        fi
    done
    rmdir "$remote_root" 2> /dev/null || true
fi

[ "${#pins[@]}" -gt 0 ] || exit 0

tmp=""
trap '[ -z "$tmp" ] || rm -rf "$tmp"' EXIT

for pin in "${pins[@]}"; do
    IFS='|' read -r name dir ref sha256 url path _file <<< "$pin"
    [ "$dir" = "${remote_root}/${name}" ] || die "${name}: unexpected fetch directory ${dir}"

    stamp="${stamp_root}/${name}.pin"
    want="${sha256} ${url} ${path}"
    if [ -f "${dir}/module.kdl" ] && [ "$(cat "$stamp" 2> /dev/null)" = "$want" ]; then
        echo "fetch-modules: ${name} ${ref} is current"
        continue
    fi

    mkdir -p build
    tmp="$(mktemp -d build/fetch-module.XXXXXX)"
    ./scripts/tect.sh fetch tree "$url" "$sha256" "$tmp" --strip-components=1

    src="$tmp"
    [ -z "$path" ] || src="${tmp}/${path}"
    [ -d "$src" ] || die "${name}: ${url} has no ${path:-module} in it"
    [ -f "${src}/module.kdl" ] || die "${name}: ${path:-the archive root} ships no module.kdl"

    rm -rf "$dir"
    mkdir -p "$(dirname "$dir")"
    cp -a "$src" "$dir"
    rm -rf "$tmp"
    tmp=""

    mkdir -p "$stamp_root"
    printf '%s\n' "$want" > "$stamp"
    echo "fetch-modules: ${name} ${ref} fetched and verified"
done
