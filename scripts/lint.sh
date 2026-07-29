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

./scripts/manifest.sh check

./scripts/gen-containerfile.sh > /dev/null
if ! git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    echo "lint: the Containerfile generates (no checkout, so no drift check)"
elif ! git ls-files --error-unmatch Containerfile.generated > /dev/null 2>&1; then
    echo "lint: Containerfile.generated is untracked, so nothing reviews it" >&2
    exit 1
elif ! git diff --quiet -- Containerfile.generated; then
    echo "lint: Containerfile.generated is stale, stage the regenerated file" >&2
    git --no-pager diff --stat -- Containerfile.generated >&2
    exit 1
else
    echo "lint: the Containerfile generates and matches the committed one"
fi

IMAGE_REGISTRY=lint.invalid ./scripts/render-iso-config.sh > /dev/null
echo "lint: installer config renders"
