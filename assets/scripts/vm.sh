#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
    echo "vm: $*" >&2
    exit 1
}

usage() {
    cat >&2 << 'EOF'
usage: scripts/vm.sh <command> <qcow2|raw|iso> [options]

  build     convert the container image into a disk image under out/
  run       boot that disk image under qemu, building it if it is missing
  spawn     boot it with systemd-vmspawn instead (qcow2 or raw)

  --image <ref>       container image to convert (default: localhost/ and
                      the plan's ungated published name)
  --tag <tag>         its tag (default: $DEFAULT_TAG, else latest)
  --target <image/flavour>
                      what a rebuild builds and what an iso installs
                      (default: the plan's ungated target)
  --rebuild           build the container image first
  --ram <size>        memory for the virtual machine (default: 8G)

Environment:
  BIB_IMAGE           bootc-image-builder image
EOF
}

command="${1:-}"
case "$command" in
    build | run | spawn) shift ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        usage
        die "unknown command '${command}'"
        ;;
esac

type="${1:-}"
shift || true
case "${command}/${type}" in
    */qcow2 | */raw | build/iso | run/iso) ;;
    spawn/iso) die "systemd-vmspawn cannot boot an installer iso" ;;
    *) die "'${command}' needs an image type (qcow2, raw or iso)" ;;
esac

image=""
tag="${DEFAULT_TAG:-latest}"
target=""
rebuild=0
ram=8G

while [ $# -gt 0 ]; do
    case "$1" in
        --image | --tag | --target | --ram)
            [ "$#" -ge 2 ] || die "$1 needs a value"
            case "$1" in
                --image) image="$2" ;;
                --tag) tag="$2" ;;
                --target) target="$2" ;;
                --ram) ram="$2" ;;
            esac
            shift 2
            ;;
        --rebuild)
            rebuild=1
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

if [ -n "$image" ]; then
    ref="${image}:${tag}"
else
    ref="$(IMAGE_REGISTRY=localhost ./scripts/tect.sh registry ref --tag "$tag")"
fi

image_file="out/${type}/disk.${type}"
[ "$type" != iso ] || image_file="out/bootiso/install.iso"

sudoif() {
    if [ "${UID}" -eq 0 ]; then
        "$@"
    elif [ -n "${SUDO_ASKPASS:-}${SSH_ASKPASS:-}" ] && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
        SUDO_ASKPASS="${SUDO_ASKPASS:-${SSH_ASKPASS}}" sudo --askpass "$@"
    else
        sudo "$@"
    fi
}

# bootc-image-builder reads the image from rootful podman
load_rootful() {
    if [ "${UID}" -eq 0 ] || [ -n "${SUDO_USER:-}" ]; then
        return 0
    fi
    if ! podman image exists "$ref"; then
        sudoif podman pull "$ref"
        return 0
    fi
    local mine theirs tmp
    mine="$(podman images --filter reference="$ref" --format '{{.ID}}')"
    theirs="$(sudoif podman images --filter reference="$ref" --format '{{.ID}}')"
    [ "$mine" != "$theirs" ] || return 0
    mkdir -p out
    tmp="$(mktemp -p "${PWD}/out" -d scp.XXXXXXXX)"
    sudoif env "TMPDIR=${tmp}" podman image scp \
        "${UID}@localhost::${ref}" "root@localhost::${ref}"
    rm -rf "$tmp"
}

build_disk() {
    local config=disk_config/disk.toml tmp args=()
    if [ "$type" = iso ]; then
        [ -z "$target" ] || args=(--target "$target")
        config="$(./scripts/render-iso-config.sh "${args[@]}")"
    fi
    load_rootful
    mkdir -p out
    tmp="$(mktemp -p "${PWD}/out" -d bib.XXXXXXXX)"
    sudoif podman run \
        --rm -it --privileged --pull=newer --net=host \
        --security-opt label=type:unconfined_t \
        -v "$(realpath "$config")":/config.toml:ro \
        -v "$tmp":/output \
        -v /var/lib/containers/storage:/var/lib/containers/storage \
        "${BIB_IMAGE:-quay.io/centos-bootc/bootc-image-builder:latest}" \
        --type "$type" --use-librepo=True --rootfs=btrfs "$ref"
    sudoif mv -f "$tmp"/* out/
    sudoif rmdir "$tmp"
    sudoif chown -R "$(id -u):$(id -g)" out/
}

run_qemu() {
    local port=8006
    while ss -tunal | grep -q ":${port} "; do
        port=$((port + 1))
    done
    echo "vm: connect to http://localhost:${port}"
    (sleep 30 && xdg-open "http://localhost:${port}") &
    podman run \
        --rm --privileged --pull=newer \
        --publish "127.0.0.1:${port}:8006" \
        --env CPU_CORES=4 \
        --env "RAM_SIZE=${ram}" \
        --env DISK_SIZE=64G \
        --env TPM=Y \
        --env GPU=Y \
        --device=/dev/kvm \
        --volume "${PWD}/${image_file}":"/boot.${type}" \
        docker.io/qemux/qemu
}

if [ "$command" = build ] || [ "$rebuild" = 1 ] || [ ! -f "$image_file" ]; then
    if [ "$rebuild" = 1 ]; then
        args=(--tag "$ref")
        [ -z "$target" ] || args+=(--target "$target")
        ./scripts/tect.sh build "${args[@]}"
    fi
    build_disk
fi

case "$command" in
    run) run_qemu ;;
    spawn)
        systemd-vmspawn \
            -M "bootc-image" \
            --console=gui \
            --cpus=2 \
            --ram="$(numfmt --from=iec "$ram")" \
            --network-user-mode \
            --vsock=false --pass-ssh-key=false \
            -i "$image_file"
        ;;
esac
