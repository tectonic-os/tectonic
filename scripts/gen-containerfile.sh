#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck disable=SC2016  # the marker is literal text, not an expansion
begin='# ---- BEGIN GENERATED (build phases and modules; see scripts/gen-containerfile.sh) ----'
end='# ---- END GENERATED ----'

skeleton=scripts/Containerfile.skeleton
out=Containerfile.generated

if ! grep -qxF "$begin" "$skeleton" || ! grep -qxF "$end" "$skeleton"; then
    echo "gen-containerfile: BEGIN/END GENERATED markers not found in ${skeleton}" >&2
    exit 1
fi

./scripts/fetch-modules.sh

section_file="$(mktemp)"
trap 'rm -f "$section_file"' EXIT
./scripts/manifest.sh section > "$section_file"

directive=""
case "$(head -1 "$skeleton")" in
    '# syntax='*) directive="$(head -1 "$skeleton")" ;;
esac

{
    [ -z "$directive" ] || echo "$directive"
    echo '# GENERATED FILE — do not edit. Produced by scripts/gen-containerfile.sh'
    echo '# from the Containerfile skeleton and modules.kdl.'
    echo
    awk -v begin="$begin" -v end="$end" -v sec="$section_file" -v directive="$directive" '
        NR == 1 && directive != "" && $0 == directive { next }
        $0 == begin {
            print
            print ""
            while ((getline sline < sec) > 0) print sline
            insection = 1
            next
        }
        $0 == end { insection = 0 }
        !insection { print }
    ' "$skeleton"
} > "$out"

echo "gen-containerfile: wrote ${out} ($(grep -c 'run-module.sh /ctx' "$out") module layers,\
 $(grep -c '^# ---- phase ' "$out") build phases)"
