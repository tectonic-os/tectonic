#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# The script runs from scripts/ or from generated/scripts/.
[ -f repo.kdl ] || cd ..

# scripts/ is absent unless the repository keeps a copy of a script.
roots=(generated/lib components)
for dir in generated/scripts scripts; do
    [ ! -d "$dir" ] || roots+=("$dir")
done
# A helper below is left to the shell it names, so it is not a script here too.
mapfile -t scripts < <(find "${roots[@]}" \
    -name '*.sh' ! -name repo.sh -type f ! \( -path '*/files/*' \
    \( -path '*/libexec/*' -o -path '*/system-generators/*' \) \) -print)
# A `repo.sh` is linted apart, because shellcheck cannot see who reads its `REPO_ID`.
mapfile -t repos < <(find components -name repo.sh -type f -print)
# What a component ships to run on the machine, and each names its shell.
mapfile -t helpers < <(find components -path '*/files/*' -type f \
    \( -path '*/libexec/*' -o -path '*/system-generators/*' \) -print)
shfmt=(shfmt -i 4 -ci -bn -sr)
counted="${#scripts[@]} scripts, ${#repos[@]} repo files and ${#helpers[@]} helpers"

if [ "${1:-}" = "--fix" ]; then
    "${shfmt[@]}" -w "${scripts[@]}" "${repos[@]}" "${helpers[@]}"
    echo "lint: formatted ${counted}"
    exit
fi

for tool in shellcheck shfmt; do
    command -v "$tool" > /dev/null 2>&1 || {
        echo "lint: $tool not found, install it first" >&2
        exit 1
    }
done

shellcheck -s bash "${scripts[@]}"
# `REPO_ID` is a `repo.sh` file's interface: the helper and the emitter read it,
# and shellcheck sees neither.
[ "${#repos[@]}" -eq 0 ] || shellcheck -s bash -e SC2034 "${repos[@]}"
[ "${#helpers[@]}" -eq 0 ] || shellcheck "${helpers[@]}"
"${shfmt[@]}" -d "${scripts[@]}" "${repos[@]}" "${helpers[@]}" || {
    echo "lint: unformatted, run ./scripts/lint.sh --fix" >&2
    exit 1
}
echo "lint: ${counted} pass shellcheck and shfmt"

./scripts/tect.sh check

./scripts/tect.sh verify
