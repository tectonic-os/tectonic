#!/bin/bash

brand_os_release() {
    local name="Tectonic" pretty_name="" default_hostname="" image_version="${IMAGE_VERSION:-dev}" arg
    for arg in "$@"; do
        case "$arg" in
            NAME=*) name="${arg#NAME=}" ;;
            PRETTY_NAME=*) pretty_name="${arg#PRETTY_NAME=}" ;;
            DEFAULT_HOSTNAME=*) default_hostname="${arg#DEFAULT_HOSTNAME=}" ;;
            IMAGE_VERSION=*) image_version="${arg#IMAGE_VERSION=}" ;;
            *)
                echo "brand_os_release: unknown argument '${arg}'" >&2
                return 1
                ;;
        esac
    done
    if [ -z "$pretty_name" ]; then
        pretty_name="Tectonic ${image_version}"
    fi
    if [ -z "$default_hostname" ]; then
        default_hostname="${name,,}"
    fi

    sed -i \
        -e "s|^NAME=.*|NAME=\"${name}\"|" \
        -e "s|^PRETTY_NAME=.*|PRETTY_NAME=\"${pretty_name}\"|" \
        -e 's|^LOGO=.*|LOGO=distributor-logo-symbolic|' \
        -e "s|^DEFAULT_HOSTNAME=.*|DEFAULT_HOSTNAME=\"${default_hostname}\"|" \
        -e 's|^HOME_URL=.*|HOME_URL="https://github.com/tectonic-os/tectonic"|' \
        -e 's|^DOCUMENTATION_URL=.*|DOCUMENTATION_URL="https://github.com/tectonic-os/tectonic"|' \
        -e 's|^SUPPORT_URL=.*|SUPPORT_URL="https://github.com/tectonic-os/tectonic/issues"|' \
        -e 's|^BUG_REPORT_URL=.*|BUG_REPORT_URL="https://github.com/tectonic-os/tectonic/issues"|' \
        /usr/lib/os-release
    if grep -q '^IMAGE_VERSION=' /usr/lib/os-release; then
        sed -i "s|^IMAGE_VERSION=.*|IMAGE_VERSION=\"${image_version}\"|" /usr/lib/os-release
    else
        echo "IMAGE_VERSION=\"${image_version}\"" >> /usr/lib/os-release
    fi
    ln -sf ../usr/lib/os-release /etc/os-release
}
