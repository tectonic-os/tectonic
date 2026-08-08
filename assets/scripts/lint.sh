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

./scripts/tect.sh check

./scripts/gen-containerfile.sh > /dev/null
mapfile -t generated < <(./scripts/tect.sh plan --json \
    | jq -r '.images[] | "containerfiles/\(.id).generated"')
if ! git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    echo "lint: the Containerfiles generate (no checkout, so no drift check)"
else
    for file in "${generated[@]}"; do
        if ! git ls-files --error-unmatch "$file" > /dev/null 2>&1; then
            echo "lint: ${file} is untracked, so nothing reviews it" >&2
            exit 1
        elif ! git diff --quiet -- "$file"; then
            echo "lint: ${file} is stale, stage the regenerated file" >&2
            git --no-pager diff --stat -- "$file" >&2
            exit 1
        fi
    done

    while IFS= read -r file; do
        [ -n "$file" ] || continue
        for want in "${generated[@]}"; do
            [ "$file" != "$want" ] || continue 2
        done
        echo "lint: ${file} belongs to no declared image, remove it" >&2
        exit 1
    done < <(git ls-files 'containerfiles/*.generated')

    echo "lint: ${#generated[@]} Containerfile(s) generate and match the committed ones"
fi

IMAGE_REGISTRY=lint.invalid ./scripts/render-iso-config.sh > /dev/null
echo "lint: installer config renders"
