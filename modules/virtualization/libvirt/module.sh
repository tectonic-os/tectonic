dnf5 install -y \
    edk2-ovmf \
    libvirt \
    libvirt-nss \
    qemu \
    qemu-img \
    qemu-system-x86-core \
    qemu-user-binfmt \
    qemu-user-static-aarch64 \
    virt-manager \
    virt-v2v \
    virt-viewer

source /ctx/lib/wrap-helpers.sh
wrap_no_hardened_malloc /usr/bin/virt-manager
