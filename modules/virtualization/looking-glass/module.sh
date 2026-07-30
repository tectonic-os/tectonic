source /ctx/lib/dkms-helpers.sh
kernel_devel_install "${DKMS_BUILD_DEPS[@]}"

git clone --quiet --depth 1 --branch "$ASSET_LOOKING_GLASS_VERSION" \
    https://github.com/gnif/LookingGlass.git /tmp/looking-glass

KVMFR_VERSION="$(dkms_conf_version /tmp/looking-glass/module)"

dkms_build_module kvmfr "$KVMFR_VERSION" /tmp/looking-glass/module

kernel_devel_remove "${DKMS_BUILD_DEPS_REMOVE[@]}"
rm -rf /tmp/looking-glass
