# Reboot this machine
[group('Power')]
reboot:
	#!/usr/bin/bash
	source /usr/share/goojust/goojust-helpers.sh
	if [[ "$(confirm "Reboot now?")" == "Proceed" ]]; then
		systemctl reboot
	else
		echo "Cancelled."
	fi

# Reboot into this device's UEFI (BIOS) setup screen
[group('Power')]
reboot-uefi:
	#!/usr/bin/bash
	source /usr/share/goojust/goojust-helpers.sh
	if [ ! -d /sys/firmware/efi ]; then
		echo "Rebooting to legacy BIOS from OS is not supported."
		exit 1
	fi
	if [[ "$(confirm "Reboot into UEFI setup?")" == "Proceed" ]]; then
		systemctl reboot --firmware-setup
	else
		echo "Cancelled."
	fi

# Shut down this machine
[group('Power')]
shutdown:
	#!/usr/bin/bash
	source /usr/share/goojust/goojust-helpers.sh
	if [[ "$(confirm "Shut down now?")" == "Proceed" ]]; then
		systemctl poweroff
	else
		echo "Cancelled."
	fi
