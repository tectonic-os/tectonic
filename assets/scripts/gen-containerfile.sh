#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck disable=SC2016  # the marker is literal text, not an expansion
begin='# ---- BEGIN GENERATED (build phases and modules; see scripts/gen-containerfile.sh) ----'
end='# ---- END GENERATED ----'

skeleton=scripts/Containerfile.skeleton
outdir=generated

if ! grep -qxF "$begin" "$skeleton" || ! grep -qxF "$end" "$skeleton"; then
    echo "gen-containerfile: BEGIN/END GENERATED markers not found in ${skeleton}" >&2
    exit 1
fi

./scripts/fetch-modules.sh
./scripts/tect.sh generate > /dev/null

directive=""
case "$(head -1 "$skeleton")" in
    '# syntax='*) directive="$(head -1 "$skeleton")" ;;
esac

section_file="$(mktemp)"
trap 'rm -f "$section_file"' EXIT

mkdir -p "$outdir"
# A Containerfile is the one thing here with no extension, so an image that is
# gone leaves as a deletion rather than as a file nothing regenerates.
find "$outdir" -maxdepth 1 -type f ! -name '*.*' -delete

mapfile -t images < <(./scripts/tect.sh plan --json | jq -r '.images[].id')
if [ "${#images[@]}" -eq 0 ]; then
    echo "gen-containerfile: no images declared, so there is nothing to generate" >&2
    exit 1
fi

for image in "${images[@]}"; do
    out="${outdir}/${image}"
    ./scripts/tect.sh section "$image" > "$section_file"

    {
        [ -z "$directive" ] || echo "$directive"
        echo '# GENERATED FILE — do not edit. Produced by scripts/gen-containerfile.sh'
        echo "# from the Containerfile skeleton and the ${image} image definition."
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

    echo "gen-containerfile: wrote ${out} ($(grep -c 'bash /ctx/module.sh' "$out") module layers,\
 $(grep -c '^# ---- phase ' "$out") build phases)"
done
