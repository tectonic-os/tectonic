# ---- /opt relocation ----
mkdir -p /usr/lib/opt
tmpfiles=/usr/lib/tmpfiles.d/zz-opt-symlinks.conf
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

# ---- module presets ----
apply_module_presets() {
    local scope="$1" dir="$2" flag=() f verb unit
    [ "$scope" = user ] && flag=(--global)
    for f in "$dir"/45-module-*.preset; do
        [ -f "$f" ] || continue
        while read -r verb unit; do
            case "$verb" in
                enable) systemctl "${flag[@]}" enable "$unit" ;;
                disable) systemctl "${flag[@]}" disable "$unit" ;;
                *) ;;
            esac
        done < "$f"
    done
}
apply_module_presets system /usr/lib/systemd/system-preset
apply_module_presets user /usr/lib/systemd/user-preset
