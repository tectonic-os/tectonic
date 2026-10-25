#!/bin/bash

install_packages() {
    local args=()
    if [ -n "${TECT_ENABLE_REPO:-}" ]; then
        args+=(--enablerepo="$TECT_ENABLE_REPO")
    fi
    dnf5 install -y "${args[@]}" "$@"
}
