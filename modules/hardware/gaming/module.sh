dnf5 install -y gamemode

source /ctx/lib/dkms-helpers.sh
kernel_devel_install "${DKMS_BUILD_DEPS[@]}" cabextract

XONE_VERSION="0.0.0+${XONE_COMMIT:0:12}"

git clone --quiet https://github.com/medusalix/xone.git /tmp/xone
git -C /tmp/xone checkout --quiet "$XONE_COMMIT"

sed -i "s/#VERSION#/${XONE_VERSION}/g" /tmp/xone/dkms.conf

dkms_build_module xone "$XONE_VERSION" /tmp/xone

install -D -m 0644 /tmp/xone/install/modprobe.conf /usr/lib/modprobe.d/xone-blacklist.conf

sh /tmp/xone/install/firmware.sh --skip-disclaimer

kernel_devel_remove "${DKMS_BUILD_DEPS_REMOVE[@]}" cabextract
rm -rf /tmp/xone
