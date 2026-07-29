# Randomize MAC address for the current Wi-Fi connection to improve privacy on public networks
[group('Network')]
randomize-mac:
	#!/usr/bin/env bash
	set -eou pipefail

	ACTIVE=$(nmcli -t -f NAME,TYPE connection show --active | awk -F: '$2=="802-11-wireless"{print $1; exit}')
	if [[ -z "$ACTIVE" ]]; then
		echo "No active Wi-Fi connection found."
		exit 1
	fi
	# DEVICE first: SSIDs may contain colons, device names never do
	DEV=$(nmcli -t -f DEVICE,TYPE connection show --active | awk -F: '$2=="802-11-wireless"{print $1; exit}')

	echo "==> Setting MAC randomization for: $ACTIVE"
	nmcli connection modify "$ACTIVE" wifi.cloned-mac-address random
	nmcli connection down "$ACTIVE"
	nmcli connection up "$ACTIVE"
	echo "Done. New MAC: $(ip link show "$DEV" | awk '/ether/{print $2}')"
