YUBIKEY_PACKAGES=(
    opensc
    pam-u2f
    pam_yubico
    pamu2fcfg
    pcsc-lite
    pcsc-lite-ccid
    yubikey-manager
)
dnf5 install -y "${YUBIKEY_PACKAGES[@]}"
