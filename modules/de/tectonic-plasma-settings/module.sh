source /ctx/lib/fetch-helpers.sh
fetch_install_rpm \
    "https://github.com/tectonic-os/plasma-bootc-updates/releases/download/v${TECTONIC_BOOTC_UPDATES_VERSION}/tectonic-bootc-updates-${TECTONIC_BOOTC_UPDATES_VERSION}-1.fc44.x86_64.rpm" \
    "$TECTONIC_BOOTC_UPDATES_SHA256"
