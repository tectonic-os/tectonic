FIRMWARE_PACKAGES=(
    iwlwifi-mvm-firmware
    iwlwifi-mld-firmware
)
dnf5 install -y "${FIRMWARE_PACKAGES[@]}"
