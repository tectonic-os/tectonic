[private]
default:
    @just --list

# Shellcheck, shfmt, rustfmt and the goldens, as CI runs it
lint:
    ./lint.sh

# Rewrite every script and the source into the format lint gates on
fix:
    ./lint.sh --fix
