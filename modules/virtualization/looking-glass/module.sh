source /ctx/lib/dkms-helpers.sh
kernel_devel_install "${DKMS_BUILD_DEPS[@]}"

git clone --quiet --depth 1 --recurse-submodules --shallow-submodules \
    --branch "$ASSET_LOOKING_GLASS_VERSION" \
    https://github.com/gnif/LookingGlass.git /tmp/looking-glass

KVMFR_VERSION="$(dkms_conf_version /tmp/looking-glass/module)"

dkms_build_module kvmfr "$KVMFR_VERSION" /tmp/looking-glass/module

LOOKING_GLASS_BUILD_DEPS=(
    cmake gcc-c++ binutils-devel
    fontconfig-devel gmp-devel nettle-devel spice-protocol libzstd-devel
    mesa-libEGL-devel mesa-libGL-devel libglvnd-devel
    libX11-devel libXfixes-devel libXi-devel libXinerama-devel
    libXScrnSaver-devel libXcursor-devel libXpresent-devel libXrandr-devel
    libxkbcommon-devel wayland-devel wayland-protocols-devel
    pipewire-devel libsamplerate-devel pulseaudio-libs-devel
)
dnf5 install -y "${LOOKING_GLASS_BUILD_DEPS[@]}"
cmake -S /tmp/looking-glass/client -B /tmp/looking-glass-build \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DCMAKE_C_FLAGS=-Wno-error=maybe-uninitialized \
    -DCMAKE_EXE_LINKER_FLAGS=-lzstd \
    -Wno-dev
cmake --build /tmp/looking-glass-build -j "$(nproc)"
cmake --install /tmp/looking-glass-build
rm -rf /tmp/looking-glass-build
dnf5 remove -y --noautoremove "${LOOKING_GLASS_BUILD_DEPS[@]}"

kernel_devel_remove "${DKMS_BUILD_DEPS_REMOVE[@]}"
rm -rf /tmp/looking-glass
