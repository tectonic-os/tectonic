dnf5 -y copr enable secureblue/packages
dnf5 -y copr disable secureblue/packages
dnf5 -y install --enablerepo="copr:copr.fedorainfracloud.org:secureblue:packages" \
    hardened_malloc \
    no_rlimit_as
