#!/bin/bash

set -ouex pipefail

rm /usr/bin/systemctl
mv /usr/bin/systemctl.bak /usr/bin/systemctl

KERNEL_PKG="$(cat /usr/lib/tectonic/kernel-package 2>/dev/null || echo 'kernel-core')"
KVER="$(rpm -q --qf '%{VERSION}-%{RELEASE}.%{ARCH}' "$KERNEL_PKG")"
DRACUT_MODULES="ostree crypt"
rpm -q plymouth &>/dev/null && DRACUT_MODULES+=" plymouth"
export DRACUT_NO_XATTR=1
dracut --force --no-hostonly --reproducible --add "$DRACUT_MODULES" \
    --kver "$KVER" \
    "/usr/lib/modules/${KVER}/initramfs.img"

mkdir -p /usr/lib/opt
tmpfiles="/usr/lib/tmpfiles.d/zz-opt-symlinks.conf"
printf 'd /var/opt 0755 root root -\n' > "$tmpfiles"
for d in /opt/*/; do
    [ -d "$d" ] || continue
    name="$(basename "$d")"
    cp -a "$d" "/usr/lib/opt/${name}"
    esc="${name// /\\x20}"
    printf 'L+ /var/opt/%s - - - - /usr/lib/opt/%s\n' "$esc" "$esc" >> "$tmpfiles"
done
rm -rf /opt
mv /opt.bak /opt

echo 'GRUB_DISABLE_OS_PROBER=false' >> /etc/default/grub

cat <<'EOF' > /tmp/composefs_execmem.te
module composefs_execmem 0.1;

require {
	type kernel_t;
	class process execmem;
}

allow kernel_t self:process execmem;
EOF
source /ctx/lib/selinux-helpers.sh
install_selinux_module /tmp/composefs_execmem.te

apply_tectonic_presets() {
    local scope="$1" dir="$2" flag=() f verb unit
    [ "$scope" = "user" ] && flag=(--global)
    for f in "$dir"/*tectonic*.preset; do
        [ -f "$f" ] || continue
        while read -r verb unit; do
            case "$verb" in
                enable) systemctl "${flag[@]}" enable "$unit" ;;
                disable) systemctl "${flag[@]}" disable "$unit" ;;
                *) ;; # comments and blank lines
            esac
        done < "$f"
    done
}
apply_tectonic_presets system /usr/lib/systemd/system-preset
apply_tectonic_presets user /usr/lib/systemd/user-preset

run_module_finalize() {
    local current_flavour="" line entry name d dir
    while IFS= read -r line; do
        entry="${line%%#*}"
        entry="${entry//[[:space:]]/}"
        [ -z "$entry" ] && continue
        if [[ "$entry" =~ ^\[([a-z][a-z0-9-]*)\]$ ]]; then
            section_name="${BASH_REMATCH[1]}"
            if [ "$section_name" = "common" ]; then
                current_flavour=""
            else
                current_flavour="$section_name"
            fi
            continue
        fi
        [ -n "$current_flavour" ] && [ "$current_flavour" != "${FLAVOUR:?}" ] && continue
        name="${entry%%@*}"
        d="/ctx/modules/${name}"
        dir=""
        [ -d "$d" ] && dir="$d"
        if [ -n "$dir" ] && [ -f "$dir/finalize.sh" ]; then
            MODDIR="$dir"; export MODDIR
            # shellcheck source=/dev/null
            source "$dir/finalize.sh"
        fi
    done < /ctx/modules.list
}
run_module_finalize
