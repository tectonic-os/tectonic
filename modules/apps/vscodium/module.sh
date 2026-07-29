dnf5 install -y --enablerepo='vscodium' codium

source /ctx/lib/wrap-helpers.sh
wrap_no_hardened_malloc /usr/share/codium/codium
