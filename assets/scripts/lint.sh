#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

mapfile -t scripts < <(
    # A `repo` file is shell and carries no extension.
    find scripts generated/lib modules -path modules/.remote -prune -o \
        \( -name '*.sh' -o -name repo \) -type f -print
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

mapfile -t repos < <(printf '%s\n' "${scripts[@]}" | grep '/repo$' || true)
mapfile -t rest < <(printf '%s\n' "${scripts[@]}" | grep -v '/repo$')
shellcheck -s bash "${rest[@]}"
# `REPO_ID` is a `repo` file's interface: the helper and the emitter read it,
# and shellcheck sees neither.
[ "${#repos[@]}" -eq 0 ] || shellcheck -s bash -e SC2034 "${repos[@]}"
"${shfmt[@]}" -d "${scripts[@]}" || {
    echo "lint: unformatted, run ./scripts/lint.sh --fix" >&2
    exit 1
}
echo "lint: ${#scripts[@]} scripts pass shellcheck and shfmt"

./scripts/tect.sh fetch modules

./scripts/tect.sh check

./scripts/tect.sh verify
