#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

mapfile -t scripts < <(find . -path ./target -prune -o -name '*.sh' -type f -print)
shfmt=(shfmt -i 4 -ci -bn -sr)

if [ "${1:-}" = "--fix" ]; then
    "${shfmt[@]}" -w "${scripts[@]}"
    cargo fmt
    echo "lint: formatted ${#scripts[@]} scripts and the source"
    exit
fi

for tool in shellcheck shfmt; do
    command -v "$tool" > /dev/null 2>&1 || {
        echo "lint: $tool not found, install it first" >&2
        exit 1
    }
done

unformatted() {
    echo "lint: unformatted, run ./lint.sh --fix" >&2
    exit 1
}

shellcheck -s bash "${scripts[@]}"
"${shfmt[@]}" -d "${scripts[@]}" || unformatted
cargo fmt --check || unformatted
echo "lint: ${#scripts[@]} scripts and the source are clean"

cargo test --quiet
echo "lint: the goldens match"
