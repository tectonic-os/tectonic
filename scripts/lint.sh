#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v shellcheck > /dev/null 2>&1; then
    echo "lint: shellcheck not found, install it first" >&2
    exit 1
fi

mapfile -t scripts < <(
    find build-phases scripts lib modules -name '*.sh' -type f
    find modules -path '*/files/*' -type f \
        \( -path '*/libexec/*' -o -path '*/system-generators/*' \)
)
shellcheck -s bash "${scripts[@]}"
echo "lint: shellcheck passed on ${#scripts[@]} scripts"

./scripts/gen-containerfile.sh > /dev/null
echo "lint: modules.list resolves"

IMAGE_REGISTRY=lint.invalid ./scripts/render-iso-config.sh > /dev/null
echo "lint: installer config renders"
