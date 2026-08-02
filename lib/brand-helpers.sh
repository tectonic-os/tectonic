#!/bin/bash

brand_os_release() {
    local name="${IMAGE_NAME:?IMAGE_NAME is unset: the image declares no name}"
    local image_version="${IMAGE_VERSION:-dev}"
    local pretty_name="${IMAGE_PRETTY_NAME:-${name} ${image_version}}"
    local default_hostname="${IMAGE_ID:-${name,,}}"

    sed -i \
        -e "s|^NAME=.*|NAME=\"${name}\"|" \
        -e "s|^PRETTY_NAME=.*|PRETTY_NAME=\"${pretty_name}\"|" \
        -e "s|^DEFAULT_HOSTNAME=.*|DEFAULT_HOSTNAME=\"${default_hostname}\"|" \
        /usr/lib/os-release

    if [ -n "${IMAGE_URL:-}" ]; then
        sed -i \
            -e "s|^HOME_URL=.*|HOME_URL=\"${IMAGE_URL}\"|" \
            -e "s|^DOCUMENTATION_URL=.*|DOCUMENTATION_URL=\"${IMAGE_URL}\"|" \
            /usr/lib/os-release
    fi
    if [ -n "${IMAGE_ISSUES_URL:-}" ]; then
        sed -i \
            -e "s|^SUPPORT_URL=.*|SUPPORT_URL=\"${IMAGE_ISSUES_URL}\"|" \
            -e "s|^BUG_REPORT_URL=.*|BUG_REPORT_URL=\"${IMAGE_ISSUES_URL}\"|" \
            /usr/lib/os-release
    fi

    if grep -q '^IMAGE_VERSION=' /usr/lib/os-release; then
        sed -i "s|^IMAGE_VERSION=.*|IMAGE_VERSION=\"${image_version}\"|" /usr/lib/os-release
    else
        echo "IMAGE_VERSION=\"${image_version}\"" >> /usr/lib/os-release
    fi
    ln -sf ../usr/lib/os-release /etc/os-release
}

install_brand_assets() {
    local logo="${IMAGE_LOGO:-}" watermark="${IMAGE_WATERMARK:-}" file

    if [ -n "$logo" ]; then
        file="$(basename "$logo")"
        install -Dm644 "/ctx/${logo}" \
            "/usr/share/icons/hicolor/scalable/places/${file}"
        sed -i "s|^LOGO=.*|LOGO=${file%.*}|" /usr/lib/os-release
    fi

    if [ -n "$watermark" ]; then
        install -Dm644 "/ctx/${watermark}" \
            "/usr/share/plymouth/themes/spinner/watermark.png"
    fi
}
