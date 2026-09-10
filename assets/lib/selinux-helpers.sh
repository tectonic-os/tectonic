#!/bin/bash
# rpm families only: this installs its tools with dnf, and a deb family
# ships the SELinux policy tooling under different package names.

ensure_checkpolicy() {
    command -v checkmodule > /dev/null 2>&1 || dnf install -y checkpolicy
}

install_selinux_module() {
    local te="$1"
    local base="${te%.te}"
    ensure_checkpolicy
    checkmodule -M -m -o "${base}.mod" "$te"
    semodule_package -o "${base}.pp" -m "${base}.mod"
    semodule -n -s targeted -X 200 -i "${base}.pp"
    rm -f "$te" "${base}.mod" "${base}.pp"
}
