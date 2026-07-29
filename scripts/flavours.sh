#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

skeleton=Containerfile.template

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
  default             the first flavour, which builds use when none is given
  installer           the flavour a fresh installer ISO lays down
  check <flavour>      succeeds if <flavour> is declared, fails loudly if not
  siblings <flavour>   every flavour except <flavour>
  image [<flavour>]    published image name for a flavour (default: default)
  images              published image name for every flavour
  cache-image         image name of the shared build cache
EOF
}

# ---- read the declared flavours ------------------------------------------
raw="$(sed -n 's/^ARG FLAVOURS="\(.*\)"$/\1/p' "$skeleton")"
[ -n "$raw" ] || die "ARG FLAVOURS not found in ${skeleton}"

flavours=()
declare -A seen=()
IFS=',' read -ra parts <<< "$raw"
for name in "${parts[@]}"; do
    name="${name//[[:space:]]/}"
    [ -n "$name" ] || continue
    [[ "$name" =~ ^[a-z][a-z0-9-]*$ ]] \
        || die "invalid flavour name '${name}' in ARG FLAVOURS (expected lowercase, digits and dashes)"
    [ -z "${seen[$name]:-}" ] || die "flavour '${name}' is listed twice in ARG FLAVOURS"
    seen["$name"]=1
    flavours+=("$name")
done
[ "${#flavours[@]}" -gt 0 ] || die "no flavours found in ARG FLAVOURS in ${skeleton}"

require_flavour() {
    local wanted="${1:-}"
    [ -n "$wanted" ] || die "expected a flavour name"
    [ -n "${seen[$wanted]:-}" ] \
        || die "'${wanted}' is not a flavour in ARG FLAVOURS in ${skeleton} (have: ${flavours[*]})"
}

# ---- commands ------------------------------------------------------------
case "${1:-}" in
    list)
        printf '%s\n' "${flavours[@]}"
        ;;
    default)
        printf '%s\n' "${flavours[0]}"
        ;;
    installer)
        [ -n "${seen[$installer]:-}" ] \
            || die "the installer flavour '${installer}' is not in ARG FLAVOURS in ${skeleton} (have: ${flavours[*]})"
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
        name="${2:-${flavours[0]}}"
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
