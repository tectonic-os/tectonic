HARDWARE_PACKAGES=(
    alsa-ucm
    alsa-utils
    dmidecode
    intel-lpmd
    lm_sensors
    lshw
    pciutils
    powerstat
)
dnf5 install -y "${HARDWARE_PACKAGES[@]}"
