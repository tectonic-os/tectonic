#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck disable=SC2016  # the backticks are literal marker text
begin='# ---- BEGIN MODULES (generated at build time from modules.list; see scripts/gen-containerfile.sh) ----'
end='# ---- END MODULES ----'

list=modules.list
skeleton=Containerfile.template
out=Containerfile.generated

# ---- read valid flavour names --------------------------------------------
flavours_out="$(./scripts/flavours.sh list)"
first_flavour="$(./scripts/flavours.sh default)"
declare -A valid_flavours=()
while IFS= read -r name; do
    valid_flavours["$name"]=1
done <<< "$flavours_out"

# ---- ARG FLAVOUR ----------------------------------------------------------
flavour_arg_emitted=0
emit_flavour_arg() {
    cat <<EOF
# ---- flavour gate ----
# Declared here rather than above: an ARG in scope is part of the cache
# key of every RUN below it, so every layer above this one is shared by
# all flavours.
ARG FLAVOUR=${first_flavour}
EOF
}

# ---- emit one module block -------------------------------------------
emit_block() {
    local name="$1" variant="$2" flavour="$3" dir
    dir="modules/${name}"
    if [ ! -d "$dir" ]; then
        echo "gen-containerfile: '${name}' does not resolve to a module directory (expected ${dir})" >&2
        exit 1
    fi

    if [ -f "${dir}/Containerfile.inc" ]; then
        if [ "$flavour_arg_emitted" = 0 ] \
            && grep -qE '\$\{?FLAVOUR\}?' "${dir}/Containerfile.inc"; then
            echo "gen-containerfile: '${name}' expands FLAVOUR in its Containerfile.inc but is listed above the first flavour-gated module, where ARG FLAVOUR is not yet declared" >&2
            exit 1
        fi
        if [ -n "$flavour" ]; then
            local part_flavour
            part_flavour="$(sed -n 's/.*FLAVOUR_GATE=\([^[:space:]]*\).*/\1/p' "${dir}/Containerfile.inc" | head -1)"
            if [ -z "$part_flavour" ]; then
                echo "gen-containerfile: '${name}' is listed under [${flavour}] but its Containerfile.inc has no FLAVOUR_GATE — the flavour gate would be silently ignored" >&2
                exit 1
            fi
            if [ "$part_flavour" != "$flavour" ]; then
                echo "gen-containerfile: '${name}' is listed under [${flavour}] but its Containerfile.inc has FLAVOUR_GATE=${part_flavour}" >&2
                exit 1
            fi
            printf '# ---- [%s] ----\n' "$flavour"
        fi
        printf '# ---- %s (verbatim from %s/Containerfile.inc) ----\n' "$name" "$dir"
        cat "${dir}/Containerfile.inc"
        return
    fi

    local env_prefix=""
    [ -n "$variant" ] && env_prefix="MODULE_VARIANT=${variant} "
    [ -n "$flavour" ] && env_prefix+="FLAVOUR_GATE=${flavour} "
    if [ -n "$flavour" ]; then
        printf '# ---- [%s] ----\n' "$flavour"
    fi
    cat <<EOF
# ---- ${name} ----
RUN --mount=type=bind,from=ctx,source=/${dir},target=/ctx/${dir} \\
    --mount=type=bind,from=ctx,source=/lib,target=/ctx/lib \\
    --mount=type=cache,target=/var/cache \\
    --mount=type=cache,target=/var/log \\
    --mount=type=tmpfs,target=/tmp \\
    ${env_prefix}bash /ctx/lib/run-module.sh /ctx/${dir}
EOF
}

# ---- parse modules.list -----------------------------------------------
current_flavour=""
while IFS= read -r line; do
    entry="${line%%#*}"
    entry="${entry//[[:space:]]/}"
    [ -z "$entry" ] && continue

    if [[ "$entry" =~ ^\[([a-z][a-z0-9-]*)\]$ ]]; then
        section_name="${BASH_REMATCH[1]}"
        if [ "$section_name" = "common" ]; then
            current_flavour=""
        else
            current_flavour="$section_name"
            if [ -z "${valid_flavours[$current_flavour]:-}" ]; then
                echo "gen-containerfile: [${current_flavour}] is not a flavour in ARG FLAVOURS in ${skeleton}" >&2
                exit 1
            fi
        fi
        continue
    fi

    name="${entry%%@*}"
    variant=""
    [ "$entry" != "$name" ] && variant="${entry#*@}"

    if [ -n "$current_flavour" ] && [ "$flavour_arg_emitted" = 0 ]; then
        section+="$(emit_flavour_arg)"$'\n\n'
        flavour_arg_emitted=1
    fi
    section+="$(emit_block "$name" "$variant" "$current_flavour")"$'\n\n'
done < "$list"

if [ "$flavour_arg_emitted" = 0 ]; then
    section+="$(emit_flavour_arg)"$'\n\n'
fi

# ---- splice into skeleton -----------------------------------------------
if ! grep -qxF "$begin" "$skeleton" || ! grep -qxF "$end" "$skeleton"; then
    echo "gen-containerfile: BEGIN/END MODULES markers not found in ${skeleton}" >&2
    exit 1
fi

directive=""
case "$(head -1 "$skeleton")" in
    '# syntax='*) directive="$(head -1 "$skeleton")" ;;
esac

section_file="$(mktemp)"
printf '%s' "$section" > "$section_file"
{
    [ -z "$directive" ] || echo "$directive"
    echo '# GENERATED FILE — do not edit. Produced by scripts/gen-containerfile.sh'
    echo '# from the Containerfile skeleton and modules.list.'
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
rm -f "$section_file"
echo "gen-containerfile: wrote ${out} ($(grep -c 'run-module.sh /ctx' "$out") module RUN layers)"
