#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

template=disk_config/iso.template.toml
out=out/iso.toml

die() {
    echo "render-iso-config: $*" >&2
    exit 1
}

usage() {
    cat >&2 << 'EOF'
usage: scripts/render-iso-config.sh [options]

  --target <image/flavour>
                    target the ISO installs (default: the plan's
                    ungated_target)
  --tag <tag>       tag it tracks (default: $DEFAULT_TAG, else latest)

Environment:
  IMAGE_REGISTRY    overrides the registry namespace the reference is under
EOF
}

target=""
tag=""

while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            [ "$#" -ge 2 ] || die "--target needs a value"
            target="$2"
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

ref=()
[ -z "$target" ] || ref+=(--target "$target")
[ -z "$tag" ] || ref+=(--tag "$tag")
IMAGE_REF="$(./scripts/tect.sh registry ref "${ref[@]}")" || die "no image reference to install"
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
