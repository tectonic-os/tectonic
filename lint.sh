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

if grep -rnE 'kdl::|Kdl[A-Z]' src --exclude-dir=parse; then
    echo "lint: a KDL type outside src/parse/, which is the only place one may appear" >&2
    exit 1
fi

if grep -rn 'ratatui' src --exclude-dir=ui; then
    echo "lint: ratatui outside src/ui/, which is the only place it may appear" >&2
    exit 1
fi

shellcheck -s bash "${scripts[@]}"
"${shfmt[@]}" -d "${scripts[@]}" || unformatted
cargo fmt --check || unformatted
echo "lint: ${#scripts[@]} scripts and the source are clean"

cargo test --quiet
echo "lint: the goldens match"
