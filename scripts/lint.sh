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

if command -v cargo > /dev/null 2>&1; then
    cargo test --quiet --manifest-path tools/manifest/Cargo.toml
    echo "lint: the goldens match"
else
    echo "lint: cargo not found, the goldens were not run" >&2
fi

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
mapfile -t generated < <(./scripts/manifest.sh plan --json \
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
