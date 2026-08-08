#!/bin/bash

MOK_KEY="/run/secrets/mok_privkey"
MOK_CERT_DER="/usr/share/secureboot/sb_cert.der"

mok_signing_available() {
    [ -s "$MOK_KEY" ] && [ -s "$MOK_CERT_DER" ]
}

configure_dkms_signing() {
    if mok_signing_available; then
        export mok_signing_key="$MOK_KEY"
        export mok_certificate="$MOK_CERT_DER"
    else
        export mok_signing_key="/run/secrets/mok_privkey"
        export mok_certificate="/run/secrets/mok_privkey.pub"
    fi
}

sign_kernel_module() {
    local ko="$1" sign_file="$2" bare
    case "$ko" in
        *.ko.xz)
            bare="${ko%.xz}"
            xz -dk "$ko"
            "$sign_file" sha256 "$MOK_KEY" "$MOK_CERT_DER" "$bare"
            xz -f "$bare"
            ;;
        *.ko.zst)
            bare="${ko%.zst}"
            zstd -dq "$ko" -o "$bare"
            "$sign_file" sha256 "$MOK_KEY" "$MOK_CERT_DER" "$bare"
            zstd -qf "$bare" -o "$ko"
            rm -f "$bare"
            ;;
        *.ko)
            "$sign_file" sha256 "$MOK_KEY" "$MOK_CERT_DER" "$ko"
            ;;
    esac
}

sign_modules_under() {
    local dir="$1" sign_file="$2"
    [ -d "$dir" ] || return 0
    while IFS= read -r ko; do
        sign_kernel_module "$ko" "$sign_file"
        echo "  Signed: $(basename "$ko")"
    done < <(find "$dir" \( -name '*.ko' -o -name '*.ko.xz' -o -name '*.ko.zst' \) 2>/dev/null)
}

sign_vmlinuz() {
    local vmlinuz="$1" cert_pem
    cert_pem=$(mktemp /tmp/mok_cert.XXXXXX.pem)
    openssl x509 -in "$MOK_CERT_DER" -inform DER -out "$cert_pem" -outform PEM
    sbsign --key "$MOK_KEY" --cert "$cert_pem" --output "${vmlinuz}.signed" "$vmlinuz"
    mv "${vmlinuz}.signed" "$vmlinuz"
    rm -f "$cert_pem"
}
