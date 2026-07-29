DEV_PACKAGES=(
    direnv
    git
    git-credential-libsecret
    git-delta
    git-lfs
)
dnf5 install -y "${DEV_PACKAGES[@]}"
