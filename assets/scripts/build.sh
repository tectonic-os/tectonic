#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

plan="$(./scripts/tect.sh plan --json)"

die() {
    echo "build: $*" >&2
    exit 1
}

usage() {
    cat >&2 << 'EOF'
usage: scripts/build.sh [options]

  --target <image/flavour>
                      what to build, e.g. tectonic/desktop; the flavour half
                      is `none` for the ungated set, which publishes
                      unsuffixed (default: the plan's default_target)
  --kernel <name>     KERNEL build arg (default: unset, the Containerfile
                      decides, which is how the kernel-freshness fallback
                      switches the whole pipeline to the stock kernel)
  --tag <ref>         tag the result; repeatable
  --secret <id>=<path>
                      mount <path> as the build secret <id>, one of the
                      IDs the plan lists for the target; repeatable
  --backend <name>    buildx or buildah (default: $BUILD_BACKEND, else
                      buildah)
  --oci-output <path> write an OCI archive here instead of loading the image
  --cache-to          export the layer cache to the registry cache repo
  --no-cache-from     do not import the registry layer cache

Environment:
  TAGS                newline-separated tags, as the metadata action emits
  LABELS              newline-separated OCI labels, same shape
  IMAGE_VERSION       stamped into the image (default: today, UTC)
  IMAGE_REGISTRY      registry holding the layer cache (default: derived
                      from the origin remote)
  MOK_KEY_PATH        shorthand for `--secret mok_privkey=<path>`, the one
                      secret a local build is likely to have
EOF
}

# ---- arguments -----------------------------------------------------------
backend="${BUILD_BACKEND:-buildah}"
target=""
kernel=""
oci_output=""
cache_from=1
cache_to=0
tags=()
labels=()
secrets=()

need_value() {
    [ "$2" -ge 2 ] || die "$1 needs a value"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            need_value "$1" "$#"
            target="$2"
            shift 2
            ;;
        --kernel)
            need_value "$1" "$#"
            kernel="$2"
            shift 2
            ;;
        --tag)
            need_value "$1" "$#"
            tags+=("$2")
            shift 2
            ;;
        --secret)
            need_value "$1" "$#"
            secrets+=("$2")
            shift 2
            ;;
        --backend)
            need_value "$1" "$#"
            backend="$2"
            shift 2
            ;;
        --oci-output)
            need_value "$1" "$#"
            oci_output="$2"
            shift 2
            ;;
        --cache-to)
            cache_to=1
            shift
            ;;
        --no-cache-from)
            cache_from=0
            shift
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

case "$backend" in
    buildx | buildah) ;;
    *) die "unknown backend '${backend}' (buildx or buildah)" ;;
esac

# ---- resolved build inputs -----------------------------------------------
target="${target:-$(jq -r '.default_target' <<< "$plan")}"
resolved="$(jq -c --arg t "$target" \
    '.images[].targets[] | select(.name == $t)' <<< "$plan")"
[ -n "$resolved" ] \
    || die "'${target}' is not a build target (have: $(
        jq -r '[.images[].targets[].name] | join(" ")' <<< "$plan"
    ))"

image="$(jq -r '.image' <<< "$resolved")"
published="$(jq -r '.published' <<< "$resolved")"
flavour_arg="$(jq -r '.flavour // ""' <<< "$resolved")"

containerfile="containerfiles/${image}.generated"

image_version="${IMAGE_VERSION:-$(date -u +%Y%m%d)}"

while IFS= read -r line; do
    if [ -n "$line" ]; then tags+=("$line"); fi
done <<< "${TAGS:-}"
[ "${#tags[@]}" -gt 0 ] \
    || tags=("${IMAGE_NAME:-$published}:${DEFAULT_TAG:-latest}")

while IFS= read -r line; do
    if [ -n "$line" ]; then labels+=("$line"); fi
done <<< "${LABELS:-}"

if [ -n "${MOK_KEY_PATH:-}" ]; then
    for pair in "${secrets[@]}"; do
        [ "${pair%%=*}" != "mok_privkey" ] \
            || die "MOK_KEY_PATH and --secret mok_privkey= both set; use one"
    done
    secrets+=("mok_privkey=${MOK_KEY_PATH}")
fi

for pair in "${secrets[@]}"; do
    case "$pair" in
        ?*=?*) ;;
        *) die "--secret takes <id>=<path>, got '${pair}'" ;;
    esac
    [ -f "${pair#*=}" ] \
        || die "secret '${pair%%=*}' points at '${pair#*=}', which does not exist"
done

# ---- registry layer cache ------------------------------------------------
cache_import_refs=()
cache_export_ref=""
if [ "$cache_from" = 1 ] || [ "$cache_to" = 1 ]; then
    if namespace="$(./scripts/registry.sh namespace)"; then
        repo="${namespace}/$(jq -r '.cache_image' <<< "$plan")"
        if [ "$cache_from" = 1 ]; then
            cache_import_refs+=("${repo}:${published}")
            while IFS= read -r sibling; do
                cache_import_refs+=("${repo}:${sibling}")
            done < <(jq -r '.siblings[].published' <<< "$resolved")
        fi
        [ "$cache_to" = 0 ] || cache_export_ref="${repo}:${published},mode=max"
    else
        [ "$cache_to" = 0 ] || die "--cache-to needs a registry namespace"
        echo "build: skipping the registry layer cache" >&2
    fi
fi

# ---- the Containerfile the build actually uses ---------------------------
./scripts/gen-containerfile.sh

# The `tect` stage copies this, and every layer mounts it from there.
install -D -m755 "$(./scripts/tect.sh --path)" build/tect

build_args=(
    "FLAVOUR=${flavour_arg}"
    "IMAGE_VERSION=${image_version}"
    "IMAGE_REGISTRY=$(./scripts/registry.sh namespace 2> /dev/null || true)"
    "CONTRACT_FILES=$(jq -r '.contract_files | join(" ")' <<< "$resolved")"
    "VERIFY_EXCEPTIONS=$(jq -r \
        '[.verify_exceptions[] | "\(.class)|\(.unit)"] | join(" ")' <<< "$resolved")"
)
[ -z "$kernel" ] || build_args+=("KERNEL=${kernel}")

echo "build: ${backend} target=${target} version=${image_version}${kernel:+ kernel=${kernel}}"
echo "build: tags ${tags[*]}"
[ "${#cache_import_refs[@]}" -eq 0 ] \
    || echo "build: importing cache from ${cache_import_refs[*]}"
[ -z "$cache_export_ref" ] || echo "build: exporting cache to ${cache_export_ref}"

# ---- backends ------------------------------------------------------------
build_buildx() {
    local args=(build --file "$containerfile")
    local arg tag label ref pair

    for arg in "${build_args[@]}"; do args+=(--build-arg "$arg"); done
    for tag in "${tags[@]}"; do args+=(--tag "$tag"); done
    for label in "${labels[@]}"; do args+=(--label "$label"); done
    for ref in "${cache_import_refs[@]}"; do
        args+=(--cache-from "type=registry,ref=${ref}")
    done
    [ -z "$cache_export_ref" ] \
        || args+=(--cache-to "type=registry,ref=${cache_export_ref}")
    for pair in "${secrets[@]}"; do
        args+=(--secret "id=${pair%%=*},src=${pair#*=}")
    done
    args+=(--provenance=false)
    [ -z "$oci_output" ] \
        || args+=(--output "type=oci,dest=${oci_output}")

    docker buildx "${args[@]}" .
}

build_buildah() {
    local args=(build --file "$containerfile")
    local arg tag label pair

    for arg in "${build_args[@]}"; do args+=(--build-arg "$arg"); done
    for tag in "${tags[@]}"; do args+=(--tag "$tag"); done
    for label in "${labels[@]}"; do args+=(--label "$label"); done
    for pair in "${secrets[@]}"; do
        args+=(--secret "id=${pair%%=*},src=${pair#*=}")
    done
    [ -z "$oci_output" ] \
        || die "the buildah backend cannot write an OCI archive"
    [ "${#cache_import_refs[@]}" -eq 0 ] \
        || echo "build: buildah ignores the registry layer cache" >&2
    [ -z "$cache_export_ref" ] \
        || die "buildah cannot export a BuildKit layer cache"

    podman "${args[@]}" --pull=newer .
}

"build_${backend}"
