#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

none=none

prefix="$(./scripts/manifest.sh image-id)"

die() {
    echo "flavours: $*" >&2
    exit 1
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/flavours.sh <command> [flavour]

All output is one item per line, in declaration order.

  list                every declared flavour
  targets             every build target: the ungated `none`, then flavours
  default             the flavour marked default in image.kdl, which
                      builds use when none is given
  pr                  the flavour a pull request builds
  check <target>      succeeds if <target> is buildable, fails loudly if not
  siblings <target>   every target except <target>
  image [<target>]    published image name (default: the default flavour)
  images              published image name for every target
  cache-image         image name of the shared build cache
EOF
}

mapfile -t flavours < <(./scripts/manifest.sh flavours)
mapfile -t targets < <(./scripts/manifest.sh targets)

declare -A buildable=()
for name in "${targets[@]}"; do
    buildable["$name"]=1
done

require_target() {
    local wanted="${1:-}"
    [ -n "$wanted" ] || die "expected a target name"
    [ -n "${buildable[$wanted]:-}" ] \
        || die "'${wanted}' is not a build target (have: ${targets[*]})"
}

image_name() {
    if [ "$1" = "$none" ]; then
        printf '%s\n' "$prefix"
    else
        printf '%s-%s\n' "$prefix" "$1"
    fi
}

case "${1:-}" in
    list)
        [ "${#flavours[@]}" -eq 0 ] || printf '%s\n' "${flavours[@]}"
        ;;
    targets)
        printf '%s\n' "${targets[@]}"
        ;;
    default)
        ./scripts/manifest.sh default-flavour
        ;;
    pr)
        ./scripts/manifest.sh pr-flavour
        ;;
    check)
        require_target "${2:-}"
        ;;
    siblings)
        require_target "${2:-}"
        for name in "${targets[@]}"; do
            [ "$name" = "$2" ] || printf '%s\n' "$name"
        done
        ;;
    image)
        name="${2:-$(./scripts/manifest.sh default-flavour)}"
        require_target "$name"
        image_name "$name"
        ;;
    images)
        for name in "${targets[@]}"; do
            image_name "$name"
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
