#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v shellcheck > /dev/null 2>&1; then
    echo "lint: shellcheck not found, install it first" >&2
    exit 1
fi

mapfile -t scripts < <(find . -path ./target -prune -o -name '*.sh' -type f -print)
shellcheck -s bash "${scripts[@]}"
echo "lint: shellcheck passed on ${#scripts[@]} scripts"

cargo test --quiet
echo "lint: the goldens match"
