#!/bin/bash

set -ouex pipefail

MODDIR="${1:?usage: run-module.sh <module dir>}"
export MODDIR

if [ -n "${FLAVOUR_GATE:-}" ]; then
    case ",${FLAVOUR_GATE}," in
        *",${FLAVOUR:-},"*) ;;
        *)
            echo "Skipping $(basename "$MODDIR"): not built for '${FLAVOUR:-the ungated build}'"
            exit 0
            ;;
    esac
fi

if [ -f "$MODDIR/repo" ]; then
    REPO_ID="$(sed -n 's/^REPO_ID="\(.*\)"/\1/p' "$MODDIR/repo")"
    if [ -n "$REPO_ID" ] && [ -f "/etc/yum.repos.d/${REPO_ID}.repo" ]; then
        echo "Repo ${REPO_ID} already configured, skipping"
    else
        # shellcheck source=/dev/null
        source "$MODDIR/repo"
    fi
fi

if [ -f "$MODDIR/versions.sh" ]; then
    # shellcheck source=/dev/null
    source "$MODDIR/versions.sh"
fi

if [ -f "$MODDIR/module.sh" ]; then
    # shellcheck source=/dev/null
    source "$MODDIR/module.sh"
fi

if [ -d "$MODDIR/selinux" ]; then
    # shellcheck source=/dev/null
    source /ctx/lib/selinux-helpers.sh
    for te in "$MODDIR"/selinux/*.te; do
        [ -f "$te" ] || continue
        cp "$te" "/tmp/$(basename "$te")"
        install_selinux_module "/tmp/$(basename "$te")"
    done
fi

if [ -d "$MODDIR/files" ]; then
    cp -rT "$MODDIR/files" /
fi

if [ -f "$MODDIR/justfile.inc" ]; then
    mkdir -p /usr/share/goojust
    cat "$MODDIR/justfile.inc" >> /usr/share/goojust/justfile.apps
fi

if [ -f "$MODDIR/flatpaks.list" ]; then
    mkdir -p /usr/share/tectonic
    cat "$MODDIR/flatpaks.list" >> /usr/share/tectonic/default-flatpaks
fi
