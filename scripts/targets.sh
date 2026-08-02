#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

none=none

die() {
    echo "targets: $*" >&2
    exit 1
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/targets.sh <command> [target]

All output is one item per line, in declaration order. A target is
<image>/<flavour>, with <image>/none for the ungated build.

  targets             every build target
  default             what a build with no target named builds
  pr                  the target a pull request builds
  ungated             the default image's ungated target, which is what
                      the installer ISO and the disk builds lay down
  check <target>      succeeds if <target> is buildable, fails loudly if not
  siblings <target>   every other target of the same image, which is every
                      target whose layers are worth importing as cache
  image [<target>]    published image name (default: the default target)
  images              published image name for every target
  cache-image         image name of the shared build cache
EOF
}

mapfile -t targets < <(./scripts/manifest.sh targets)
[ "${#targets[@]}" -gt 0 ] || die "nothing is buildable; no image is declared"

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
    local image="${1%%/*}" flavour="${1#*/}"
    if [ "$flavour" = "$none" ]; then
        printf '%s\n' "$image"
    else
        printf '%s-%s\n' "$image" "$flavour"
    fi
}

case "${1:-}" in
    targets)
        printf '%s\n' "${targets[@]}"
        ;;
    default)
        ./scripts/manifest.sh default-target
        ;;
    pr)
        ./scripts/manifest.sh pr-target
        ;;
    ungated)
        printf '%s/%s\n' "$(./scripts/manifest.sh default-image)" "$none"
        ;;
    check)
        require_target "${2:-}"
        ;;
    siblings)
        require_target "${2:-}"
        for name in "${targets[@]}"; do
            [ "$name" = "$2" ] && continue
            [ "${name%%/*}" = "${2%%/*}" ] || continue
            printf '%s\n' "$name"
        done
        ;;
    image)
        name="${2:-$(./scripts/manifest.sh default-target)}"
        require_target "$name"
        image_name "$name"
        ;;
    images)
        for name in "${targets[@]}"; do
            image_name "$name"
        done
        ;;
    cache-image)
        printf '%s-cache\n' "$(./scripts/manifest.sh default-image)"
        ;;
    *)
        usage
        exit 1
        ;;
esac
