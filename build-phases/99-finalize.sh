#!/bin/bash

set -ouex pipefail

rm /usr/bin/systemctl
mv /usr/bin/systemctl.bak /usr/bin/systemctl

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

apply_module_presets() {
    local scope="$1" dir="$2" flag=() f verb unit
    [ "$scope" = "user" ] && flag=(--global)
    for f in "$dir"/45-module-*.preset; do
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
apply_module_presets system /usr/lib/systemd/system-preset
apply_module_presets user /usr/lib/systemd/user-preset

run_module_finalize() {
    local entry name gate dir entries=()
    read -ra entries <<< "${FINALIZE_ORDER:-}"
    for entry in "${entries[@]}"; do
        name="${entry%%:*}"
        gate=""
        [ "$entry" = "$name" ] || gate="${entry#*:}"
        [ -z "$gate" ] || [ "$gate" = "${FLAVOUR:-}" ] || continue
        dir="/ctx/modules/${name}"
        MODDIR="$dir"; export MODDIR
        # shellcheck source=/dev/null
        source "$dir/finalize.sh"
    done
}
run_module_finalize
