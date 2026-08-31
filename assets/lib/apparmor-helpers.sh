#!/bin/bash
# deb families only. Not the shape of selinux-helpers.sh: a .te is source that
# has to be compiled, where a profile is text that only has to be valid.

install_apparmor_profile() {
    local profile="$1"
    if ! command -v apparmor_parser > /dev/null 2>&1; then
        echo "apparmor_parser is not in this image, so a shipped profile cannot be checked" >&2
        return 1
    fi
    # -Q parses without loading, so no kernel interface is needed. It writes
    # "Cache read/write disabled" to stderr on the way, which is not an error.
    apparmor_parser -Q "$profile"
    install -D -m 0644 -- "$profile" "/etc/apparmor.d/$(basename "$profile")"
}
