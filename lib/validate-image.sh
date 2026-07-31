#!/bin/bash

set -euo pipefail

failures=0
fail() {
	echo "FAIL: $*" >&2
	failures=$((failures + 1))
}

echo "==> bootc install print-configuration"
if bootc install print-configuration >/dev/null; then
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
	while read -r type path _ _ _ _ target; do
		case "$type" in
		L+ | L)
			target="${target//\\x20/ }"
			if [ ! -e "$target" ]; then
				fail "${path} -> ${target}: target does not exist"
			else
				echo "    ${path} -> ${target} ok"
			fi
			;;
		esac
	done <"$tmpfiles"
else
	echo "    (no /usr/lib/opt symlinks declared)"
fi

echo "==> contract files"
if [ -z "${CONTRACT_FILES:-}" ]; then
	echo "    (none declared)"
else
	# shellcheck disable=SC2086
	set -- $CONTRACT_FILES
	for path in "$@"; do
		if [ -e "$path" ]; then
			echo "    ${path} ok"
		else
			fail "${path}: the manifest declares it, the image does not have it"
		fi
	done
fi

enablement_links() {
	local root="$1" unit="$2" link target
	[ -d "$root" ] || return 0
	while IFS= read -r link; do
		[ -n "$link" ] || continue
		target="$(readlink "$link")"
		[ "$target" = /dev/null ] && continue
		if [ "${link##*/}" = "$unit" ] || [ "${target##*/}" = "$unit" ]; then
			printf '%s\n' "$link"
		fi
	done < <(find "$root" -maxdepth 2 -type l 2>/dev/null || true)
}

verify_allowed_patterns=(
	'Failed to create .*: Unit [^ ]+\.(mount|swap) not found\.$'
	"Command 'man [^']*' failed with code [0-9]+\$"
)
verify_allowed="$(
	IFS='|'
	printf '%s' "${verify_allowed_patterns[*]}"
)"
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

			config_root="/etc/systemd/${scope}"
			links="$(enablement_links "$config_root" "$unit")"
			if [ "$verb" = "enable" ] && [ -z "$links" ]; then
				fail "${unit}: preset enables it, but nothing under ${config_root} does"
			elif [ "$verb" = "disable" ] && [ -n "$links" ]; then
				fail "${unit}: preset disables it, but ${config_root} still enables it:" \
					"$(echo "$links" | tr '\n' ' ')"
			fi

			if [ "$scope" = "system" ] && [ "$verb" = "enable" ]; then
				if out="$(systemd-analyze verify --no-pager "$unit" 2>&1)"; then
					echo "        ${unit} enabled"
				elif [ -z "${out//[[:space:]]/}" ]; then
					fail "${unit}: systemd-analyze verify failed without saying why"
				else
					unexpected="$(printf '%s\n' "$out" |
						grep -Ev "$verify_allowed" |
						grep -Ev '^[[:space:]]*$' || true)"
					if [ -n "$unexpected" ]; then
						fail "${unit}: systemd-analyze verify"
						# shellcheck disable=SC2001
						echo "$unexpected" | sed 's/^/          /' >&2
					else
						echo "        ${unit} enabled (verify: mount/swap notes only)"
					fi
				fi
			else
				echo "        ${unit} ${verb}d"
			fi
		done <"$preset"
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
