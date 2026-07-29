#!/bin/bash
# shellcheck disable=SC2034  # the dep arrays are consumed by the sourcing scripts

source "$(dirname "${BASH_SOURCE[0]}")/kernel-helpers.sh"
source "$(dirname "${BASH_SOURCE[0]}")/sign-helpers.sh"

DKMS_BUILD_DEPS=(dkms gcc make git sbsigntools openssl)
DKMS_BUILD_DEPS_REMOVE=(dkms gcc make sbsigntools)

dkms_conf_version() {
    local version
    version="$(sed -n 's/^PACKAGE_VERSION="\([^"]*\)"/\1/p' "$1/dkms.conf")"
    if [ -z "$version" ]; then
        echo "no PACKAGE_VERSION in $1/dkms.conf" >&2
        return 1
    fi
    echo "$version"
}

dkms_build_module() {
    local name="$1" version="$2" src="$3"
    local kver
    kver="$(kver)"

    configure_dkms_signing
    if ! mok_signing_available; then
        echo "No MOK key supplied, ${name} modules are unsigned."
    fi

    rm -rf "/usr/src/${name}-${version}"
    cp -a "$src" "/usr/src/${name}-${version}"
    dkms add -m "$name" -v "$version"
    dkms build -m "$name" -v "$version" -k "$kver"
    dkms install -m "$name" -v "$version" -k "$kver" --force

    rm -f /var/lib/dkms/mok.key /var/lib/dkms/mok.pub
    rm -rf "/var/lib/dkms/${name}" "/usr/src/${name}-${version}"
}
