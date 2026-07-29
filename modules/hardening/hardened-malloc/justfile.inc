# Exempt an app from the system-wide hardened_malloc LD_PRELOAD (fixes crashes in some GTK/Electron apps)
[group('Hardening')]
hardened-malloc-exempt app:
	#!/usr/bin/env bash
	set -eou pipefail
	app="{{ app }}"

	fixed=0
	sysdesk="/usr/share/applications/${app}.desktop"
	userdesk="$HOME/.local/share/applications/${app}.desktop"

	if [[ -f "$sysdesk" ]]; then
		mkdir -p "$HOME/.local/share/applications"
		sed 's/^Exec=/Exec=env -u LD_PRELOAD /' "$sysdesk" > "$userdesk"
		echo "==> Wrote launcher override: $userdesk"
		fixed=1
	elif [[ -f "$userdesk" ]]; then
		if grep -q '^Exec=env -u LD_PRELOAD ' "$userdesk"; then
			echo "==> Already exempted: $userdesk"
		else
			sed -i 's/^Exec=/Exec=env -u LD_PRELOAD /' "$userdesk"
			echo "==> Patched launcher in place: $userdesk"
			echo "    App updates may rewrite this file (e.g. Gear Lever), re-run after updating."
		fi
		fixed=1
	else
		echo "No desktop entry ${app}.desktop, candidates:"
		{ ls /usr/share/applications; ls "$HOME/.local/share/applications" 2>/dev/null; } | grep -i -- "$app" | sort -u || echo "  (none)"
	fi
	if [[ "$fixed" == 1 ]]; then
		update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
		command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 2>/dev/null || true
	fi

	if [[ -x "/usr/bin/${app}" ]]; then
		shim="$HOME/.local/bin/${app}"
		mkdir -p "$HOME/.local/bin"
		printf '#!/usr/bin/env bash\nexec env -u LD_PRELOAD /usr/bin/%s "$@"\n' "$app" > "$shim"
		chmod 755 "$shim"
		echo "==> Wrote CLI shim: $shim"
		fixed=1
	fi

	if [[ "$fixed" == 0 ]]; then
		echo "Nothing exempted, no desktop entry or /usr/bin binary matched '${app}'."
		exit 1
	fi
	echo "Done. Consider baking a wrapper into the image if this exemption proves permanent."

# Remove a hardened_malloc exemption created by hardened-malloc-exempt
[group('Hardening')]
hardened-malloc-unexempt app:
	#!/usr/bin/env bash
	set -eou pipefail
	app="{{ app }}"

	removed=0
	dest="$HOME/.local/share/applications/${app}.desktop"
	if [[ -f "$dest" ]] && grep -q 'env -u LD_PRELOAD' "$dest"; then
		if [[ -f "/usr/share/applications/${app}.desktop" ]]; then
			rm "$dest"
			echo "==> Removed launcher override: $dest"
		else
			sed -i 's/^Exec=env -u LD_PRELOAD /Exec=/' "$dest"
			echo "==> Restored launcher: $dest"
		fi
		update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
		command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 2>/dev/null || true
		removed=1
	fi
	shim="$HOME/.local/bin/${app}"
	if [[ -f "$shim" ]] && grep -q 'env -u LD_PRELOAD' "$shim"; then
		rm "$shim"
		echo "==> Removed CLI shim: $shim"
		removed=1
	fi
	[[ "$removed" == 1 ]] || echo "No exemption found for '${app}'."

# List hardened_malloc exemptions (user overrides and image-baked wrappers)
[group('Hardening')]
hardened-malloc-exemptions:
	#!/usr/bin/env bash
	echo "User launcher overrides:"
	grep -l 'env -u LD_PRELOAD' "$HOME/.local/share/applications"/*.desktop 2>/dev/null || echo "  (none)"
	echo "User CLI shims:"
	grep -l 'env -u LD_PRELOAD' "$HOME/.local/bin"/* 2>/dev/null || echo "  (none)"
	echo "Baked into the image:"
	grep -rl 'env -u LD_PRELOAD' /usr/bin /usr/share/codium/codium 2>/dev/null || echo "  (none)"
