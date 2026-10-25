# ---- tmpfiles helpers ----
# At boot, systemd-tmpfiles splits a line on whitespace and unquotes each field.
# It then expands `%` specifiers in each field.
# A path has to be written escaped to survive both steps.
tmpfiles_escape() {
    local s=$1
    s=${s//\\/\\x5c}
    s=${s//%/%%}
    s=${s//$'\t'/\\x09}
    s=${s//$'\n'/\\x0a}
    s=${s//$'\r'/\\x0d}
    s=${s//$'\v'/\\x0b}
    s=${s//$'\f'/\\x0c}
    s=${s//\'/\\x27}
    s=${s//\"/\\x22}
    s=${s// /\\x20}
    printf '%s' "$s"
}

# The map holds every path that a tmpfiles.d file already declares, as written.
# systemd warns about a duplicate line and drops one of the two.
declare -A tmpfiles_declared=()
read_declared() {
    local verb path
    tmpfiles_declared=()
    while read -r verb path _; do
        case "$verb" in '' | \#*) continue ;; esac
        [ -n "$path" ] && tmpfiles_declared[$path]=1
    done < <(cat /usr/lib/tmpfiles.d/*.conf /etc/tmpfiles.d/*.conf 2> /dev/null)
}

# Records an escaped path in the map.
# It fails if a tmpfiles.d file already declares the path.
claim() {
    [ -z "${tmpfiles_declared[$1]:-}" ] || return 1
    tmpfiles_declared[$1]=1
}

# A regular file left in /var does not reach the image.
# A relocated entry is kept in /usr/lib for that reason.
# The line makes systemd-tmpfiles link the copy into /var at boot.
# If a later build runs finalize over this image, then its copy replaces the
# copy of this build. The link line of this build already serves that copy.
relocate() {
    local src=$1 store=$2 link=$3 verb=$4 conf=$5 esc
    rm -rf "$store"
    mkdir -p "$(dirname "$store")"
    cp -a "$src" "$store"
    esc="$(tmpfiles_escape "$link")"
    claim "$esc" || return 0
    printf '%s %s - - - - %s\n' "$verb" "$esc" "$(tmpfiles_escape "$store")" >> "$conf"
}

declare_dir() {
    local esc
    esc="$(tmpfiles_escape "$1")"
    claim "$esc" || return 0
    printf 'd %s 0755 root root -\n' "$esc" >> "$2"
}

# The removal at the end of each relocation section takes hidden entries too.
# The globs match them for that reason.
shopt -s dotglob

# ---- /opt relocation ----
tmpfiles=/usr/lib/tmpfiles.d/zz-opt-symlinks.conf
read_declared
declare_dir /var/opt "$tmpfiles"
for d in /opt/*/; do
    [ -d "$d" ] || continue
    name="$(basename "$d")"
    relocate "$d" "/usr/lib/opt/${name}" "/var/opt/${name}" L+ "$tmpfiles"
done
rm -rf /opt
mv /opt.bak /opt

# ---- /usr/local relocation ----
# The build root moved /usr/local aside only where the base links it into /var.
# On the running system the user writes to /var/usrlocal.
# Each directory under /usr/local therefore stays a real directory there.
# Only the entries inside a directory link to /usr/lib/usrlocal.
# `L+` would remove a file or directory that the user made at a link path.
# The lines use `L` for that reason.
if [ -L /usr/local.bak ]; then
    tmpfiles=/usr/lib/tmpfiles.d/zz-usrlocal-symlinks.conf
    read_declared
    declare_dir /var/usrlocal "$tmpfiles"
    for entry in /usr/local/*; do
        [ -e "$entry" ] || [ -L "$entry" ] || continue
        name="$(basename "$entry")"
        if [ -d "$entry" ] && [ ! -L "$entry" ]; then
            for child in "$entry"/*; do
                [ -e "$child" ] || [ -L "$child" ] || continue
                declare_dir "/var/usrlocal/${name}" "$tmpfiles"
                rel="${name}/$(basename "$child")"
                relocate "$child" "/usr/lib/usrlocal/${rel}" "/var/usrlocal/${rel}" L "$tmpfiles"
            done
            continue
        fi
        relocate "$entry" "/usr/lib/usrlocal/${name}" "/var/usrlocal/${name}" L "$tmpfiles"
    done
    # shellcheck disable=SC2114 # relocate already copied every entry of this /usr/local
    rm -rf /usr/local
    mv /usr/local.bak /usr/local
fi
shopt -u dotglob

# ---- component presets ----
apply_component_presets() {
    local scope="$1" dir="$2" flag=() f verb unit
    [ "$scope" = user ] && flag=(--global)
    for f in "$dir"/45-component-*.preset; do
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
apply_component_presets system /usr/lib/systemd/system-preset
apply_component_presets user /usr/lib/systemd/user-preset

# ---- /var to tmpfiles ----
# `rpm-ostree` walks /var at base compose time and writes these rules; nothing
# regenerates them for a derived layer. A directory this build leaves in /var is
# applied once at provisioning and never updated by `bootc upgrade`. Runs last,
# after every hook: a directory a later step made would not be recorded.
var_conf=/usr/lib/tmpfiles.d/zz-var-autogen.conf

# /var/cache and /var/log are cache mounts on this layer — busy, and not ours.
# /var/run is the symlink out to /run and stock systemd already declares it.
# /var/lib/dpkg is the package database itself on the deb bases.
var_keep=(/var/cache /var/log /var/run /var/lib/dpkg)

# If a later build runs finalize over this image, then zz-var-autogen.conf
# already holds the lines of this build. read_declared reads them back. The
# later build then keeps the lines whose directories this build removed.
touch "$var_conf"
read_declared

var_prune=()
for keep in "${var_keep[@]}"; do
    var_prune+=(-path "$keep" -o)
done
unset 'var_prune[-1]'

# Pre-order, so a directory's own line lands above the lines below it.
var_remove=()
while IFS=$'\t' read -r -d '' type mode owner group path; do
    esc="$(tmpfiles_escape "$path")"
    # Already declared by another tmpfiles file: that is the base's own /var,
    # the lint does not flag it, and something later in the build may still need
    # it on disk — the rechunk runs `rpm-ostree`, which writes to /var/tmp.
    claim "$esc" || continue
    var_remove+=("$path")
    case "$type" in
        d) printf 'd %s %s %s %s -\n' "$esc" "$mode" "$owner" "$group" >> "$var_conf" ;;
        l) printf 'L+ %s - - - - %s\n' "$esc" "$(tmpfiles_escape "$(readlink "$path")")" >> "$var_conf" ;;
        # A regular file: a tmpfiles rule reconstructs a directory, not a
        # file's content. Dropped, which is what bootc's own converter does.
        *) ;;
    esac
done < <(find /var -mindepth 1 -xdev \( "${var_prune[@]}" \) -prune -o -printf '%y\t%m\t%u\t%g\t%p\0')

# Only what was recorded above, deepest first since find listed parents before
# children. `rmdir`, so a directory still holding something that was skipped
# survives. `rm -rf` would take it down with everything in it.
for ((i = ${#var_remove[@]} - 1; i >= 0; i--)); do
    path="${var_remove[i]}"
    if [ -d "$path" ] && [ ! -L "$path" ]; then
        rmdir --ignore-fail-on-non-empty "$path"
    else
        rm -f "$path"
    fi
done
