#!/bin/bash

install_packages() {
    local args=()
    if [ -n "${TECT_ENABLE_REPO:-}" ]; then
        args+=(--enablerepo="$TECT_ENABLE_REPO")
    fi
    dnf5 install -y "${args[@]}" "$@"
}

install_groups() {
    local args=()
    if [ -n "${TECT_ENABLE_REPO:-}" ]; then
        args+=(--enablerepo="$TECT_ENABLE_REPO")
    fi
    dnf5 group install -y "${args[@]}" "$@"
}
