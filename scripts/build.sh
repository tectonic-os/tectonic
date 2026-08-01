#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

containerfile=Containerfile.generated

image_id="$(./scripts/manifest.sh image-id)"

# renovate: datasource=docker depName=docker.io/moby/buildkit
buildkit_image="docker.io/moby/buildkit:v0.31.2"
buildkit_container="${image_id}-buildkitd"
buildkit_volume="${image_id}-buildkit"
buildkit_label="${image_id}.buildkitd"
buildkit_context=/build
buildkit_secret_dir=/run/secrets

die() {
	echo "build: $*" >&2
	exit 1
}

usage() {
	cat >&2 <<'EOF'
usage: scripts/build.sh [options]

  --flavour <name>     target to build: a flavour, or `none` for the
                      ungated set published unsuffixed (default:
                      scripts/flavours.sh default)
  --kernel <name>     KERNEL build arg (default: unset, the Containerfile
                      decides, which is how the kernel-freshness fallback
                      switches the whole pipeline to the stock kernel)
  --tag <ref>         tag the result; repeatable
  --secret <id>=<path>
                      mount <path> as the build secret <id>, one of the
                      IDs `scripts/manifest.sh secrets` lists; repeatable
  --backend <name>    buildkit, buildx or buildah (default: $BUILD_BACKEND,
                      else buildkit)
  --oci-output <path> write an OCI archive here instead of loading the image
  --cache-to          export the layer cache to the registry cache repo
  --no-cache-from     do not import the registry layer cache
  --reset             remove the BuildKit daemon and its cache volume,
                      then exit; the next build starts cold

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
backend="${BUILD_BACKEND:-buildkit}"
flavour=""
kernel=""
oci_output=""
cache_from=1
cache_to=0
reset=0
tags=()
labels=()
secrets=()

need_value() {
	[ "$2" -ge 2 ] || die "$1 needs a value"
}

while [ $# -gt 0 ]; do
	case "$1" in
	--flavour)
		need_value "$1" "$#"
		flavour="$2"
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
	--reset)
		reset=1
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
buildkit | buildx | buildah) ;;
*) die "unknown backend '${backend}' (buildkit, buildx or buildah)" ;;
esac

if [ "$reset" = 1 ]; then
	podman rm --force "$buildkit_container" >/dev/null 2>&1 || true
	podman volume rm --force "$buildkit_volume" >/dev/null 2>&1 || true
	echo "build: removed the buildkit daemon and its cache volume"
	exit 0
fi

# ---- resolved build inputs -----------------------------------------------
flavour="${flavour:-$(./scripts/flavours.sh default)}"
./scripts/flavours.sh check "$flavour"

flavour_arg="$flavour"
[ "$flavour" != none ] || flavour_arg=""

image_version="${IMAGE_VERSION:-$(date -u +%Y%m%d)}"

while IFS= read -r line; do
	if [ -n "$line" ]; then tags+=("$line"); fi
done <<<"${TAGS:-}"
[ "${#tags[@]}" -gt 0 ] || tags=("${IMAGE_NAME:-$image_id}:${DEFAULT_TAG:-latest}")

while IFS= read -r line; do
	if [ -n "$line" ]; then labels+=("$line"); fi
done <<<"${LABELS:-}"

if [ -n "${MOK_KEY_PATH:-}" ]; then
	for pair in "${secrets[@]}"; do
		[ "${pair%%=*}" != "mok_privkey" ] ||
			die "MOK_KEY_PATH and --secret mok_privkey= both set; use one"
	done
	secrets+=("mok_privkey=${MOK_KEY_PATH}")
fi

for pair in "${secrets[@]}"; do
	case "$pair" in
	?*=?*) ;;
	*) die "--secret takes <id>=<path>, got '${pair}'" ;;
	esac
	[ -f "${pair#*=}" ] ||
		die "secret '${pair%%=*}' points at '${pair#*=}', which does not exist"
done

# ---- registry layer cache ------------------------------------------------
cache_import_refs=()
cache_export_ref=""
if [ "$cache_from" = 1 ] || [ "$cache_to" = 1 ]; then
	if repo="$(./scripts/registry.sh ref "$(./scripts/flavours.sh cache-image)")"; then
		if [ "$cache_from" = 1 ]; then
			cache_import_refs+=("${repo}:${flavour}")
			while IFS= read -r sibling; do
				cache_import_refs+=("${repo}:${sibling}")
			done < <(./scripts/flavours.sh siblings "$flavour")
		fi
		[ "$cache_to" = 0 ] || cache_export_ref="${repo}:${flavour},mode=max"
	else
		[ "$cache_to" = 0 ] || die "--cache-to needs a registry namespace"
		echo "build: skipping the registry layer cache" >&2
	fi
fi

# ---- the Containerfile the build actually uses ---------------------------
./scripts/gen-containerfile.sh

build_args=(
	"FLAVOUR=${flavour_arg}"
	"IMAGE_VERSION=${image_version}"
	"IMAGE_REGISTRY=$(./scripts/registry.sh namespace 2>/dev/null || true)"
	"CONTRACT_FILES=$(./scripts/manifest.sh contract-files "$flavour" | tr '\n' ' ')"
	"VERIFY_EXCEPTIONS=$(./scripts/manifest.sh verify-exceptions "$flavour" | tr '\n' ' ')"
)
[ -z "$kernel" ] || build_args+=("KERNEL=${kernel}")

echo "build: ${backend} flavour=${flavour} version=${image_version}${kernel:+ kernel=${kernel}}"
echo "build: tags ${tags[*]}"
[ "${#cache_import_refs[@]}" -eq 0 ] ||
	echo "build: importing cache from ${cache_import_refs[*]}"
[ -z "$cache_export_ref" ] || echo "build: exporting cache to ${cache_export_ref}"

# ---- backends ------------------------------------------------------------
buildkitd_ensure() {
	local run_args=(
		--detach
		--name "$buildkit_container"
		--privileged
		--security-opt label=disable
		--volume "${buildkit_volume}:/var/lib/buildkit"
		--volume "${PWD}:${buildkit_context}:ro"
	)
	local pair
	for pair in "${secrets[@]}"; do
		run_args+=(--volume "${pair#*=}:${buildkit_secret_dir}/${pair%%=*}:ro")
	done

	local want have
	want="$(printf '%s\n' "$buildkit_image" "${run_args[@]}" | sha256sum | cut -d' ' -f1)"
	have="$(podman inspect --format \
		"{{index .Config.Labels \"${buildkit_label}\"}} {{.State.Running}}" \
		"$buildkit_container" 2>/dev/null || true)"
	[ "$have" = "${want} true" ] && return 0

	podman rm --force "$buildkit_container" >/dev/null 2>&1 || true
	podman run "${run_args[@]}" --label "${buildkit_label}=${want}" \
		"$buildkit_image" >/dev/null

	for _ in $(seq 30); do
		if podman exec "$buildkit_container" buildctl debug workers >/dev/null 2>&1; then
			return 0
		fi
		sleep 1
	done
	podman logs "$buildkit_container" >&2 || true
	die "buildkitd did not come up"
}

local_ref() {
	local first="${1%%/*}"
	if [ "$first" != "$1" ]; then
		case "$first" in
		*.* | *:* | localhost)
			printf '%s\n' "$1"
			return
			;;
		esac
	fi
	printf 'localhost/%s\n' "$1"
}

build_buildkit() {
	buildkitd_ensure

	local args=(
		build
		--frontend dockerfile.v0
		--local "context=${buildkit_context}"
		--local "dockerfile=${buildkit_context}"
		--opt "filename=${containerfile}"
	)
	local arg label ref tag first pair

	for arg in "${build_args[@]}"; do args+=(--opt "build-arg:${arg}"); done
	for label in "${labels[@]}"; do args+=(--opt "label:${label}"); done
	for ref in "${cache_import_refs[@]}"; do
		args+=(--import-cache "type=registry,ref=${ref}")
	done
	[ -z "$cache_export_ref" ] ||
		args+=(--export-cache "type=registry,ref=${cache_export_ref}")
	for pair in "${secrets[@]}"; do
		args+=(--secret "id=${pair%%=*},src=${buildkit_secret_dir}/${pair%%=*}")
	done

	if [ -n "$oci_output" ]; then
		podman exec "$buildkit_container" buildctl "${args[@]}" \
			--output "type=oci,name=${tags[0]}" >"$oci_output"
		return
	fi

	first="$(local_ref "${tags[0]}")"
	podman exec "$buildkit_container" buildctl "${args[@]}" \
		--output "type=docker,name=${first}" | podman load --quiet
	for tag in "${tags[@]:1}"; do
		podman tag "$first" "$(local_ref "$tag")"
	done
}

build_buildx() {
	local args=(build --file "$containerfile")
	local arg tag label ref pair

	for arg in "${build_args[@]}"; do args+=(--build-arg "$arg"); done
	for tag in "${tags[@]}"; do args+=(--tag "$tag"); done
	for label in "${labels[@]}"; do args+=(--label "$label"); done
	for ref in "${cache_import_refs[@]}"; do
		args+=(--cache-from "type=registry,ref=${ref}")
	done
	[ -z "$cache_export_ref" ] ||
		args+=(--cache-to "type=registry,ref=${cache_export_ref}")
	for pair in "${secrets[@]}"; do
		args+=(--secret "id=${pair%%=*},src=${pair#*=}")
	done
	args+=(--provenance=false)
	[ -z "$oci_output" ] ||
		args+=(--output "type=oci,dest=${oci_output}")

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
	[ -z "$oci_output" ] ||
		die "the buildah backend cannot write an OCI archive"
	[ "${#cache_import_refs[@]}" -eq 0 ] ||
		echo "build: buildah ignores the registry layer cache" >&2
	[ -z "$cache_export_ref" ] ||
		die "buildah cannot export a BuildKit layer cache"

	podman "${args[@]}" --pull=newer .
}

"build_${backend}"
