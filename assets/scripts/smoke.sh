#!/usr/bin/env bash
set -euo pipefail

# The boot path CI proves an image on: installed to a raw disk as the recipe
# says, booted under qemu, and handed back once ssh answers. The smoke test and
# the collection's machine scan both call it.
cd "$(dirname "$0")/.."

# The renderer a deb image ships and a fedora one does not.
RENDERER=/usr/libexec/grub-menu-from-bls
PORT=2222

die() {
    echo "smoke: $*" >&2
    exit 1
}

[ "$#" -eq 2 ] || die "usage: scripts/smoke.sh <image-ref in root's store> <work dir>"
ref="$1"
dir="$(realpath -m "$2")"
mkdir -p "$dir"
for tool in jq qemu-system-x86_64 ssh ssh-keygen; do
    command -v "$tool" > /dev/null 2>&1 || die "$tool is not installed"
done

recipe="$(./scripts/tect.sh recipe)"
filesystem="$(jq -r .filesystem <<< "$recipe")"
composefs="$(jq -r .composeFsBackend <<< "$recipe")"
bootloader="$(jq -r .bootloader <<< "$recipe")"

rm -f "${dir}/key" "${dir}/key.pub" "${dir}/disk.raw"
ssh-keygen -q -t ed25519 -f "${dir}/key" -N '' -C smoke
truncate -s "${DISK_SIZE:-20G}" "${dir}/disk.raw"

# `--generic-image` whatever the recipe says: this disk boots under another
# firmware than the one that installed it, so it wants the removable path and
# no NVRAM entry. The assertions arrive over ssh, and an image may ship sshd
# disabled. The last `console=` is /dev/console, so ttyS0 goes after tty0.
args=(
    --via-loopback --generic-image --filesystem "$filesystem"
    --karg console=tty0 --karg 'console=ttyS0,115200n8'
    --karg systemd.wants=sshd.service
)
# fisherman's reading of the recipe: empty and `grub2` are bootc's default.
case "$bootloader" in
    "" | grub2) ;;
    *) args+=(--bootloader "$bootloader") ;;
esac
mounts=()
# `--composefs-backend` refuses a containers-storage source with `Invalid
# splitstream content type`, and takes an OCI layout.
if [ "$composefs" = true ]; then
    sudo rm -rf "${dir}/oci"
    sudo podman push -q "$ref" "oci:${dir}/oci"
    mounts=(-v "${dir}/oci:/oci:ro")
    args+=(--composefs-backend --source-imgref oci:/oci)
fi
sudo podman run --rm --privileged --pull=never --pid=host --net=host \
    --security-opt label=type:unconfined_t \
    -v /dev:/dev \
    -v /var/lib/containers/storage:/var/lib/containers/storage \
    -v "${dir}:/output" \
    "${mounts[@]}" \
    "$ref" bootc install to-disk "${args[@]}" /output/disk.raw

# vm.sh's `render_menu`, over the same two layouts: nothing renders a deb
# image's menu during an install, and its GRUB reads no BLS entries.
render_menu() {
    local lo target device root="" status=0
    target="$(mktemp -d)"
    mkdir "${target}/boot"
    lo="$(sudo losetup --show -fP "${dir}/disk.raw")"
    for device in "${lo}"p*; do
        [ -b "$device" ] || continue
        sudo mount "$device" "${target}/boot" 2> /dev/null || continue
        if [ -d "${target}/boot/loader/entries" ]; then
            root=/target
            break
        fi
        if [ -d "${target}/boot/boot/loader/entries" ]; then
            root=/target/boot
            break
        fi
        sudo umount "${target}/boot"
    done
    if [ -z "$root" ]; then
        sudo losetup -d "$lo"
        die "no partition of the disk carries \`loader/entries\`"
    fi
    sudo podman run --rm --net=none --security-opt label=disable \
        -v "${target}:/target" --entrypoint "" "$ref" \
        /bin/sh -c "test -x ${RENDERER} || exit 3; exec ${RENDERER} ${root}" || status=$?
    sudo umount "${target}/boot"
    sudo losetup -d "$lo"
    rmdir "${target}/boot" "$target"
    case "$status" in
        0) echo "smoke: rendered the boot menu" ;;
        3) ;;
        *) die "${RENDERER} failed: exit ${status}" ;;
    esac
}
render_menu
sudo chown "$(id -u):$(id -g)" "${dir}/disk.raw"

# Secure Boot is on, under Microsoft's keys as on a bought machine, unless
# SECURE_BOOT=0 declares a chain that boots without it.
secure_boot="${SECURE_BOOT:-1}"
if [ "$secure_boot" = 1 ]; then
    firmware=(/usr/share/OVMF/OVMF_CODE_4M.secboot.fd:/usr/share/OVMF/OVMF_VARS_4M.ms.fd
        /usr/share/edk2/ovmf/OVMF_CODE.secboot.fd:/usr/share/edk2/ovmf/OVMF_VARS.secboot.fd)
    machine=(-machine 'q35,smm=on' -global 'driver=cfi.pflash01,property=secure,value=on')
else
    firmware=(/usr/share/OVMF/OVMF_CODE_4M.fd:/usr/share/OVMF/OVMF_VARS_4M.fd
        /usr/share/edk2/ovmf/OVMF_CODE.fd:/usr/share/edk2/ovmf/OVMF_VARS.fd)
    machine=(-machine q35)
fi
code=""
for pair in "${firmware[@]}"; do
    [ -f "${pair%%:*}" ] && [ -f "${pair#*:}" ] || continue
    code="${pair%%:*}"
    cp "${pair#*:}" "${dir}/vars.fd"
    break
done
[ -n "$code" ] || die "no OVMF firmware for SECURE_BOOT=${secure_boot} is installed"
# A hardened image refuses root over ssh. The drop-in reaches the machine as a
# credential and lives in /run, so the /etc a scan measures is the image's.
# The key is a credential too: a composefs install drops
# `--root-ssh-authorized-keys`.
# shellcheck disable=SC2016 # expanded by systemd, from each family's EnvironmentFile
dropin="$(printf '%s\n' '[Service]' 'ExecStart=' \
    'ExecStart=/usr/sbin/sshd -D -oPermitRootLogin=prohibit-password $OPTIONS $SSHD_OPTS' \
    | base64 -w0)"
qemu-system-x86_64 \
    -smbios "type=11,value=io.systemd.credential.binary:ssh.authorized_keys.root=$(base64 -w0 "${dir}/key.pub")" \
    -smbios "type=11,value=io.systemd.credential.binary:systemd.unit-dropin.sshd.service=${dropin}" \
    -smbios "type=11,value=io.systemd.credential.binary:systemd.unit-dropin.ssh.service=${dropin}" \
    "${machine[@]}" -m "${RAM:-4096}" -smp 2 -cpu host -enable-kvm -display none \
    -drive "if=pflash,unit=0,format=raw,readonly=on,file=${code}" \
    -drive "if=pflash,unit=1,format=raw,file=${dir}/vars.fd" \
    -serial "file:${dir}/console.log" \
    -drive "file=${dir}/disk.raw,if=virtio,format=raw" \
    -nic "user,hostfwd=tcp:127.0.0.1:${PORT}-:22" \
    -daemonize -pidfile "${dir}/qemu.pid" \
    > "${dir}/qemu.log" 2>&1

on_machine() {
    ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -o ConnectTimeout=5 -o LogLevel=ERROR -i "${dir}/key" -p "$PORT" root@localhost "$@"
}
for i in $(seq 60); do
    if on_machine true 2> /dev/null; then
        echo "smoke: ssh answered after ${i} attempts"
        # The fifth byte of the variable is the state; the first four are its attributes.
        state="$(on_machine od -An -tu1 -j4 -N1 \
            /sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c | tr -d ' ')"
        [ "$state" = "$secure_boot" ] \
            || die "the machine reads Secure Boot as '${state}' where SECURE_BOOT is ${secure_boot}"
        exit 0
    fi
    sleep 10
done
die "ssh did not answer in ten minutes; the serial console is ${dir}/console.log"
