#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
    echo "registry: $*" >&2
    exit 1
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/registry.sh <command> [name]

  namespace       registry and owner, e.g. ghcr.io/someone
  ref <name>      <namespace>/<name>

Environment:
  IMAGE_REGISTRY  overrides the namespace; CI sets it from the workflow
                  context. Otherwise it is derived from the origin remote.
EOF
}

namespace() {
    local registry="${IMAGE_REGISTRY:-}" url owner
    if [ -z "$registry" ]; then
        url="$(git config --get remote.origin.url 2> /dev/null || true)"
        owner="$(printf '%s\n' "$url" \
            | sed -n 's#^\(git@github\.com:\|ssh://git@github\.com/\|https://github\.com/\)\([^/]*\)/.*#\2#p')"
        [ -n "$owner" ] \
            || die "no IMAGE_REGISTRY set and no github origin remote to derive one from"
        registry="ghcr.io/${owner}"
    fi
    printf '%s\n' "${registry,,}"
}

case "${1:-}" in
    namespace)
        namespace
        ;;
    ref)
        if [ "$#" -lt 2 ] || [ -z "$2" ]; then
            die "ref needs an image name"
        fi
        ns="$(namespace)"
        printf '%s/%s\n' "$ns" "$2"
        ;;
    *)
        usage
        exit 1
        ;;
esac
