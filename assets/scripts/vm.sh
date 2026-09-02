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
  --installer <which> what converts the image into a disk: `bib` for
                      bootc-image-builder, `bootc` for `bootc install to-disk`.
                      `tect vm` passes this off the target's base family; the
                      default here is bib, which is fedora-only.

Environment:
  BIB_IMAGE           bootc-image-builder image
  DISK_SIZE           size of the disk `--installer bootc` writes (default: 20G).
                      The file is sparse, so this is a ceiling rather than a cost.
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
installer=bib

while [ $# -gt 0 ]; do
    case "$1" in
        --image | --tag | --target | --ram | --installer)
            [ "$#" -ge 2 ] || die "$1 needs a value"
            case "$1" in
                --image) image="$2" ;;
                --tag) tag="$2" ;;
                --target) target="$2" ;;
                --ram) ram="$2" ;;
                --installer)
                    case "$2" in
                        bib | bootc) installer="$2" ;;
                        *) die "--installer is bib or bootc, not '$2'" ;;
                    esac
                    ;;
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

# A registry `bootc install --composefs-backend` can pull from, stood up for
# this install and torn down after it. Measured 2026-09-02: the composefs pull
# refuses any locally built image — `Invalid splitstream content type` — and
# `--source-imgref containers-storage:` refuses identically, so it is about how
# the image is stored rather than how it is named. A registry is the only way
# in, and the objective is that nobody types a `podman run`, so this owns one.
registry_up() {
    local port=5000
    while ss -tunal | grep -q ":${port} "; do
        port=$((port + 1))
    done
    registry_port="$port"
    registry_id="$(podman run --rm --detach --publish "127.0.0.1:${port}:5000" \
        docker.io/library/registry:2)"
    # The push below fails on a registry that is not listening yet.
    local waited=0
    until curl -sf -m 2 "http://localhost:${port}/v2/" > /dev/null; do
        waited=$((waited + 1))
        [ "$waited" -lt 60 ] || die "the registry did not come up on :${port}"
        sleep 0.5
    done
}

registry_down() {
    [ -z "${registry_id:-}" ] || podman rm -f "$registry_id" > /dev/null 2>&1 || true
    registry_id=""
}

# HUP as well as INT and TERM: bash runs no EXIT trap for an untrapped fatal
# signal, so closing the terminal mid-install would leave the container holding
# its port for good. The signal handlers exit rather than returning, since a
# `registry_down` that fell through would carry on with no registry.
arm_registry_trap() {
    trap registry_down EXIT
    trap 'registry_down; exit 130' INT TERM HUP
}

disarm_registry_trap() {
    trap - EXIT INT TERM HUP
}

# `bootc install to-disk`, for a family bootc-image-builder cannot convert. It
# writes a raw disk to a file with no loop device of its own: `--via-loopback`
# is what removes the `losetup -P` the recipe used to need.
install_disk() {
    if [ "$type" = iso ]; then
        die "an installer iso is Anaconda's, and no deb family carries it"
    fi
    command -v curl > /dev/null 2>&1 || die "curl is not installed, and this needs it"
    if [ "$type" = qcow2 ]; then
        command -v qemu-img > /dev/null 2>&1 \
            || die "qemu-img is not installed, and a qcow2 is converted from the raw disk"
    fi

    # Everything is written beside the disk and moved onto it at the end, so a
    # run that fails or is interrupted leaves the disk that was already there.
    # Truncating in place would replace a bootable disk with a sparse hole that
    # both this script and `tect vm` then read as finished.
    local raw="out/${type}/disk.raw.part" pushed
    mkdir -p "out/${type}"
    rm -f "$raw"
    truncate -s "${DISK_SIZE:-20G}" "$raw"

    arm_registry_trap
    registry_up
    pushed="localhost:${registry_port}/$(basename "${ref%:*}"):${ref##*:}"
    podman push --tls-verify=false "$ref" "$pushed"

    # --pull=always because root's store keeps the ref from a previous install,
    # which is what silently ran a ten-minute-old image three times running.
    # --pid=host and --net=host are carried from the recipe this was measured
    # with and are not individually justified here: the push is reachable
    # without --net=host, since a rootless --publish binds in the host netns,
    # but whether bootc's own fetch needs it was never separated out. Do not
    # drop them as tidying without re-running an install.
    sudoif podman run \
        --rm --privileged --pull=always --tls-verify=false \
        --pid=host --net=host \
        -v /var/lib/containers:/var/lib/containers \
        -v /dev:/dev \
        -v "${PWD}/out/${type}":/out \
        --security-opt label=type:unconfined_t \
        "$pushed" \
        bootc install to-disk --via-loopback --composefs-backend \
        --filesystem ext4 --wipe --generic-image /out/disk.raw.part
    registry_down
    disarm_registry_trap

    sudoif chown -R "$(id -u):$(id -g)" out/
    # bootc writes raw; a qcow2 is a conversion on top of it. Either way the
    # finished article is moved into place as the last thing that happens.
    if [ "$type" = qcow2 ]; then
        qemu-img convert -O qcow2 "$raw" "${image_file}.part"
        rm -f "$raw"
        mv -f "${image_file}.part" "$image_file"
    else
        mv -f "$raw" "$image_file"
    fi
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
    case "$installer" in
        bib) build_disk ;;
        *) install_disk ;;
    esac
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
