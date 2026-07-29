BACKUP_PACKAGES=(
    borgbackup
    rclone
    restic
)
dnf5 install -y "${BACKUP_PACKAGES[@]}"
