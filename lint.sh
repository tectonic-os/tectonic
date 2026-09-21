#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# `modules/` here is a fetched collection cache, which is linted where it is
# authored.
mapfile -t scripts < <(find . \( -path ./target -o -path ./modules \) -prune -o \
    -name '*.sh' -type f -print)
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

if grep -rnE 'kdl::|Kdl[A-Z]' src --exclude-dir=parse --exclude-dir=provenance; then
    echo "lint: a KDL type outside the two places that read a document, src/parse/ and the \
readers in src/provenance/" >&2
    exit 1
fi

if grep -nE 'kdl::|Kdl[A-Z]' src/provenance/mod.rs; then
    echo "lint: a KDL type in src/provenance/mod.rs, which holds the model half" >&2
    exit 1
fi

# Every member's sources, so a crate added to the workspace later is covered
# without editing this. ratatui belongs to `common` and to nothing here.
mapfile -t member_src < <(find . \( -path ./target -o -path ./modules \) -prune \
    -o -path '*/src/*' -name '*.rs' -type f -print)

if grep -n 'ratatui' "${member_src[@]}"; then
    echo "lint: ratatui outside the common crate, which is the only place it may appear" >&2
    exit 1
fi

# Three alternatives, because one pattern does not see every form. `common::ui`
# catches a path and a renaming import, the braced form catches `ui` imported
# beside another component of the shared crate, and `ui::` catches a call site
# whose import sits in another file.
if grep -rnE 'common::ui|common::\{[^}]*\bui\b|\bui::' src/resolve src/emit; then
    echo "lint: the resolver or an emitter reading the ui module, and that dependency runs one way" >&2
    exit 1
fi

# The rule above is textual and scoped to two directories, so a re-export of the
# module under another name anywhere else walks around it: `pub use common::ui
# as shim` in `copy.rs`, then `shim::form` in an emitter, and neither tree holds
# the string `ui`. Re-exporting the module is never needed. Re-exporting one
# item out of it is, so `pub use common::ui::tree::Change` must still pass.
if grep -rnE '^[[:space:]]*pub(\(crate\))? use common::ui( as [A-Za-z_][A-Za-z0-9_]*)?[[:space:]]*;' src; then
    echo "lint: the ui module re-exported under another name, which walks around the rule above" >&2
    exit 1
fi

# `install.sh` is what a stranger pipes into `sh`, so it is checked as the
# shell it declares. Every other script here is bash.
mapfile -t posix < <(grep -lx '#!/bin/sh' "${scripts[@]}")
mapfile -t in_bash < <(grep -Lx '#!/bin/sh' "${scripts[@]}")
shellcheck -s bash "${in_bash[@]}"
[ "${#posix[@]}" -eq 0 ] || shellcheck -s sh "${posix[@]}"
"${shfmt[@]}" -d "${scripts[@]}" || unformatted
cargo fmt --check || unformatted
echo "lint: ${#scripts[@]} scripts and the source are clean"

cargo test --quiet
echo "lint: the goldens match"
