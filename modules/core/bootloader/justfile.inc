# Regenerate grub.cfg with os-prober so other installed OSes (e.g. Windows) show up in the boot menu
[group('System')]
regenerate-grub:
	sudo /usr/libexec/grub2-os-prober-regen
