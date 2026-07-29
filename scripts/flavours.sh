#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

prefix=tectonic

installer=laptop

die() {
    echo "flavours: $*" >&2
    exit 1
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/flavours.sh <command> [flavour]

All output is one item per line, in declaration order.

  list                every flavour
  default             the flavour marked default in modules.kdl, which
                      builds use when none is given
  pr                  the flavour a pull request builds
  installer           the flavour a fresh installer ISO lays down
  check <flavour>      succeeds if <flavour> is declared, fails loudly if not
  siblings <flavour>   every flavour except <flavour>
  image [<flavour>]    published image name for a flavour (default: default)
  images              published image name for every flavour
  cache-image         image name of the shared build cache
EOF
}

mapfile -t flavours < <(./scripts/manifest.sh flavours)
[ "${#flavours[@]}" -gt 0 ] || die "no flavours declared in modules.kdl"

declare -A seen=()
for name in "${flavours[@]}"; do
    seen["$name"]=1
done

require_flavour() {
    local wanted="${1:-}"
    [ -n "$wanted" ] || die "expected a flavour name"
    [ -n "${seen[$wanted]:-}" ] \
        || die "'${wanted}' is not a flavour in modules.kdl (have: ${flavours[*]})"
}

case "${1:-}" in
    list)
        printf '%s\n' "${flavours[@]}"
        ;;
    default)
        ./scripts/manifest.sh default-flavour
        ;;
    pr)
        ./scripts/manifest.sh pr-flavour
        ;;
    installer)
        [ -n "${seen[$installer]:-}" ] \
            || die "the installer flavour '${installer}' is not in modules.kdl (have: ${flavours[*]})"
        printf '%s\n' "$installer"
        ;;
    check)
        require_flavour "${2:-}"
        ;;
    siblings)
        require_flavour "${2:-}"
        for name in "${flavours[@]}"; do
            [ "$name" = "$2" ] || printf '%s\n' "$name"
        done
        ;;
    image)
        name="${2:-$(./scripts/manifest.sh default-flavour)}"
        require_flavour "$name"
        printf '%s-%s\n' "$prefix" "$name"
        ;;
    images)
        for name in "${flavours[@]}"; do
            printf '%s-%s\n' "$prefix" "$name"
        done
        ;;
    cache-image)
        printf '%s-cache\n' "$prefix"
        ;;
    *)
        usage
        exit 1
        ;;
esac
