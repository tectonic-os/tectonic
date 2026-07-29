#!/bin/bash

install_selinux_module() {
    local te="$1"
    local base="${te%.te}"
    checkmodule -M -m -o "${base}.mod" "$te"
    semodule_package -o "${base}.pp" -m "${base}.mod"
    semodule -n -s targeted -X 200 -i "${base}.pp"
    rm -f "$te" "${base}.mod" "${base}.pp"
}
