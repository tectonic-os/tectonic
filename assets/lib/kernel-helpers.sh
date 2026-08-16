#!/bin/bash
# rpm families only: `kver` asks rpm, and the DKMS build below it assumes
# the kernel-devel layout that goes with it.

kver() {
    local pkg
    pkg="$(cat /usr/lib/kernel-build/kernel-package 2> /dev/null || echo 'kernel-core')"
    rpm -q --qf '%{VERSION}-%{RELEASE}.%{ARCH}' "$pkg"
}
