#!/bin/bash

set -ouex pipefail

KERNEL_PKG="$(cat /usr/lib/kernel-build/kernel-package 2>/dev/null || echo 'kernel-core')"
KVER="$(rpm -q --qf '%{VERSION}-%{RELEASE}.%{ARCH}' "$KERNEL_PKG")"

DRACUT_MODULES=(ostree crypt)
COLLECTED="/usr/lib/kernel-build/dracut.modules"
if [ -f "$COLLECTED" ]; then
    while IFS= read -r name; do
        [[ -z "$name" || "$name" == \#* ]] && continue
        DRACUT_MODULES+=("$name")
    done < "$COLLECTED"
fi

export DRACUT_NO_XATTR=1
dracut --force --no-hostonly --reproducible \
    --add "${DRACUT_MODULES[*]}" \
    --kver "$KVER" \
    "/usr/lib/modules/${KVER}/initramfs.img"

rm -f /usr/libexec/kernel-devel-helpers.sh
