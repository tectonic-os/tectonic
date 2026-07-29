# Show BIOS info
[group('System')]
bios-info:
	#!/usr/bin/bash
	sudo bash <<'EOF'
	echo "Manufacturer: $(dmidecode -s baseboard-manufacturer)"
	echo "Product Name: $(dmidecode -s baseboard-product-name)"
	echo "Version: $(dmidecode -s bios-version)"
	echo "Release Date: $(dmidecode -s bios-release-date)"
	EOF

# Measure idle power draw
[group('System')]
check-idle-power-draw:
	#!/usr/bin/bash
	sudo powerstat -a -r

# Regenerate grub.cfg with os-prober so other installed OSes (e.g. Windows) show up in the boot menu
[group('System')]
regenerate-grub:
	sudo /usr/libexec/grub2-os-prober-regen
