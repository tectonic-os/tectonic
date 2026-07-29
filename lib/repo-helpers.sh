#!/bin/bash

add_disabled_repo() {
    dnf5 config-manager addrepo --from-repofile="$1"
    dnf5 config-manager setopt "${REPO_ID:?add_disabled_repo needs REPO_ID set}.enabled=0"
}
