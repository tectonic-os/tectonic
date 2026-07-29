NETWORK_PACKAGES=(
    avahi-gobject
    avahi-tools
    cifs-utils
    nmap-ncat
    tcpdump
    traceroute
    wireguard-tools
)
dnf5 install -y "${NETWORK_PACKAGES[@]}"
