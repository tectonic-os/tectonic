dnf5 group install -y kde-desktop

dnf5 group install -y networkmanager-submodules

dnf5 remove -y --noautoremove \
    abrt abrt-addon-ccpp abrt-addon-kerneloops abrt-addon-pstoreoops \
    abrt-addon-vmcore abrt-addon-xorg abrt-dbus abrt-desktop abrt-gui \
    abrt-plugin-bodhi gnome-abrt \
    PackageKit PackageKit-command-not-found PackageKit-glib \
    plasma-discover plasma-discover-libs plasma-discover-packagekit \
    plasma-discover-offline-updates \
    sssd-ad sssd-common-pac sssd-ipa sssd-krb5 sssd-ldap \
    kdump-utils kexec-tools makedumpfile \
    qemu-user-static-alpha qemu-user-static-arm qemu-user-static-hexagon \
    qemu-user-static-hppa qemu-user-static-loongarch64 qemu-user-static-m68k \
    qemu-user-static-microblaze qemu-user-static-mips qemu-user-static-or1k \
    qemu-user-static-ppc qemu-user-static-riscv qemu-user-static-s390x \
    qemu-user-static-sh4 qemu-user-static-sparc qemu-user-static-x86 \
    qemu-user-static-xtensa \
    NetworkManager-l2tp NetworkManager-libreswan NetworkManager-pptp \
    NetworkManager-cloud-setup NetworkManager-tui \
    plasma-nm-l2tp plasma-nm-openswan plasma-nm-pptp

dnf5 install -y plasma-browser-integration

dnf5 install -y plymouth plymouth-theme-spinner
plymouth-set-default-theme spinner

KDE_PACKAGES=(
    gvfs
    gvfs-client
    gvfs-fuse
    input-remapper
    kamera
    kate
    kate-krunner-plugin
    kate-plugins
    ksystemlog
    plasma-firewall
    plasma-firewall-firewalld
)
dnf5 install -y "${KDE_PACKAGES[@]}"

dnf5 -y copr enable ublue-os/packages
dnf5 -y copr disable ublue-os/packages
dnf5 -y install --enablerepo="copr:copr.fedorainfracloud.org:ublue-os:packages" \
    krunner-bazaar
