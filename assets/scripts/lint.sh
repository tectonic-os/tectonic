#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

mapfile -t scripts < <(
    find scripts generated/lib modules -path modules/.remote -prune -o \
        -name '*.sh' -type f -print
    find modules -path modules/.remote -prune -o -path '*/files/*' -type f \
        \( -path '*/libexec/*' -o -path '*/system-generators/*' \) -print
)
shfmt=(shfmt -i 4 -ci -bn -sr)

if [ "${1:-}" = "--fix" ]; then
    "${shfmt[@]}" -w "${scripts[@]}"
    echo "lint: formatted ${#scripts[@]} scripts"
    exit
fi

for tool in shellcheck shfmt; do
    command -v "$tool" > /dev/null 2>&1 || {
        echo "lint: $tool not found, install it first" >&2
        exit 1
    }
done

shellcheck -s bash "${scripts[@]}"
"${shfmt[@]}" -d "${scripts[@]}" || {
    echo "lint: unformatted, run ./scripts/lint.sh --fix" >&2
    exit 1
}
echo "lint: ${#scripts[@]} scripts pass shellcheck and shfmt"

./scripts/tect.sh fetch modules

./scripts/tect.sh check

./scripts/tect.sh verify
