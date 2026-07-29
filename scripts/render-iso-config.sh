#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

template=disk_config/iso.template.toml
out=build/iso.generated.toml

die() {
    echo "render-iso-config: $*" >&2
    exit 1
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/render-iso-config.sh [options]

  --flavour <name>   target the ISO installs (default: the ungated build)
  --tag <tag>       tag it tracks (default: $DEFAULT_TAG, else latest)

Environment:
  IMAGE_REGISTRY    registry namespace, as scripts/registry.sh reads it
EOF
}

flavour=""
tag="${DEFAULT_TAG:-latest}"

while [ $# -gt 0 ]; do
    case "$1" in
        --flavour)
            [ "$#" -ge 2 ] || die "--flavour needs a value"
            flavour="$2"
            shift 2
            ;;
        --tag)
            [ "$#" -ge 2 ] || die "--tag needs a value"
            tag="$2"
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage
            die "unknown argument '$1'"
            ;;
    esac
done

flavour="${flavour:-none}"
./scripts/flavours.sh check "$flavour"

IMAGE_REF="$(./scripts/registry.sh ref "$(./scripts/flavours.sh image "$flavour")"):${tag}"
export IMAGE_REF

command -v envsubst > /dev/null 2>&1 \
    || die "envsubst not found; install gettext-envsubst (Fedora) or gettext-base (Debian)"

mkdir -p "$(dirname "$out")"
{
    echo '# GENERATED FILE, do not edit. Produced by scripts/render-iso-config.sh'
    echo '# from disk_config/iso.template.toml.'
    # shellcheck disable=SC2016  # the allowlist is a literal name, not an expansion
    envsubst '${IMAGE_REF}' < "$template"
} > "$out"

# shellcheck disable=SC2016  # a literal pattern, not an expansion
if grep -n '\${' "$out" >&2; then
    die "unsubstituted \${...} above in ${out}; the template may only use \${IMAGE_REF}"
fi

echo "render-iso-config: wrote ${out} (${IMAGE_REF})" >&2
printf '%s\n' "${PWD}/${out}"
