#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v shellcheck > /dev/null 2>&1; then
    echo "lint: shellcheck not found, install it first" >&2
    exit 1
fi

./scripts/fetch-modules.sh

mapfile -t scripts < <(
    find build-phases scripts lib modules -path modules/.remote -prune -o \
        -name '*.sh' -type f -print
    find modules -path modules/.remote -prune -o -path '*/files/*' -type f \
        \( -path '*/libexec/*' -o -path '*/system-generators/*' \) -print
)
shellcheck -s bash "${scripts[@]}"
echo "lint: shellcheck passed on ${#scripts[@]} scripts"

./scripts/manifest.sh check

shell_classes="$(sed -n 's/^\t\[\([a-z-]*\)\]=.*/\1/p' lib/validate-image.sh | sort)"
parser_classes="$(sed -n 's/^const VERIFY_CLASSES[^=]*= \[\(.*\)\];$/\1/p' \
    tools/manifest/src/module.rs | tr -d '" ' | tr ',' '\n' | sort)"
if [ -z "$shell_classes" ]; then
    echo "lint: no verify classes found in lib/validate-image.sh" >&2
    exit 1
elif [ -z "$parser_classes" ]; then
    echo "lint: no VERIFY_CLASSES found in tools/manifest/src/module.rs" >&2
    exit 1
elif [ "$shell_classes" != "$parser_classes" ]; then
    echo "lint: the verify diagnostic classes disagree" >&2
    diff <(echo "$shell_classes") <(echo "$parser_classes") |
        sed 's/^</  only in validate-image.sh: /; s/^>/  only in module.rs:        /' >&2
    exit 1
fi
echo "lint: verify classes agree ($(echo "$shell_classes" | tr '\n' ' ' | sed 's/ *$//'))"

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
