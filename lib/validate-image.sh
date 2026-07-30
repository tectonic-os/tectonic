#!/bin/bash

set -euo pipefail

failures=0
fail() {
    echo "FAIL: $*" >&2
    failures=$((failures + 1))
}

echo "==> bootc install print-configuration"
if bootc install print-configuration > /dev/null; then
    echo "    ok"
else
    fail "bootc install print-configuration failed to parse"
fi

echo "==> initramfs"
kernel_pkg="$(cat /usr/lib/tectonic/kernel-package 2>/dev/null || echo 'kernel-core')"
if kver="$(rpm -q --qf '%{VERSION}-%{RELEASE}.%{ARCH}' "$kernel_pkg" 2>/dev/null)"; then
    initramfs="/usr/lib/modules/${kver}/initramfs.img"
    if [ -f "$initramfs" ]; then
        echo "    ${initramfs} present"
    else
        fail "initramfs missing at ${initramfs}"
    fi
else
    fail "cannot determine kernel version from package ${kernel_pkg}"
fi

echo "==> /usr/lib/opt symlinks"
tmpfiles="/usr/lib/tmpfiles.d/zz-opt-symlinks.conf"
if [ -f "$tmpfiles" ]; then
    while read -r type path _ _ _ target _; do
        case "$type" in
            L+|L)
                target="${target//\\x20/ }"
                if [ ! -e "$target" ]; then
                    fail "${path} -> ${target}: target does not exist"
                else
                    echo "    ${path} -> ${target} ok"
                fi
                ;;
        esac
    done < "$tmpfiles"
else
    echo "    (no /usr/lib/opt symlinks declared)"
fi

echo "==> expected binaries"
for bin in bootc systemctl rpm-ostree; do
    if command -v "$bin" > /dev/null 2>&1; then
        echo "    ${bin} ok"
    else
        fail "${bin} not on PATH"
    fi
done

echo "==> systemd unit verification"
checked=0
for scope in system user; do
    unit_dirs="/usr/lib/systemd/${scope} /etc/systemd/${scope}"
    for preset in "/usr/lib/systemd/${scope}-preset/"*tectonic*.preset; do
        [ -f "$preset" ] || continue
        echo "    ${preset}"
        while read -r verb unit; do
            case "$verb" in
                enable | disable) ;;
                *) continue ;;
            esac
            checked=$((checked + 1))

            # shellcheck disable=SC2086
            unit_file="$(find ${unit_dirs} -name "${unit}" -print -quit 2>/dev/null || true)"
            if [ -z "$unit_file" ] || [ ! -f "$unit_file" ]; then
                if [ "$verb" = "enable" ]; then
                    fail "${unit}: unit file not found in ${unit_dirs}"
                else
                    echo "        ${unit} (not present, ${verb}d)"
                fi
                continue
            fi

            if [ "$scope" = "system" ] && [ "$verb" = "enable" ]; then
                if out="$(systemd-analyze verify --no-pager "$unit" 2>&1)"; then
                    echo "        ${unit} ok"
                else
                    echo "        ${unit} verify notes:"
                    # shellcheck disable=SC2001
                    echo "$out" | sed 's/^/          /' >&2 || true
                fi
            else
                echo "        ${unit} ok (exists)"
            fi
        done < "$preset"
    done
done

if [ "$checked" -eq 0 ]; then
    fail "no tectonic preset files found"
fi

echo
if [ "$failures" -eq 0 ]; then
    echo "All validation checks passed."
else
    echo "${failures} validation check(s) failed." >&2
    exit 1
fi
