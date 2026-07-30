#!/bin/bash

set -ouex pipefail

KERNEL_PKG="$(cat /usr/lib/tectonic/kernel-package 2>/dev/null || echo 'kernel-core')"
KVER="$(rpm -q --qf '%{VERSION}-%{RELEASE}.%{ARCH}' "$KERNEL_PKG")"

export DRACUT_NO_XATTR=1
dracut --force --no-hostonly --reproducible \
    --add "ostree crypt" \
    --kver "$KVER" \
    "/usr/lib/modules/${KVER}/initramfs.img"
