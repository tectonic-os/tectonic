[private]
default:
    @just --list

# Check Just Syntax
[group('Just')]
check:
    just --unstable --fmt --check -f Justfile

# Fix Just Syntax
[group('Just')]
fix:
    just --unstable --fmt -f Justfile

# Shellcheck over the scaffolding and the goldens over the binary, as CI runs it
lint:
    ./lint.sh

# Runs shfmt on all Bash scripts
format:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v shfmt &> /dev/null; then
        echo "shfmt could not be found. Please install it." >&2
        exit 1
    fi
    find . -path ./target -prune -o -name '*.sh' -type f -exec shfmt --write '{}' ';'
