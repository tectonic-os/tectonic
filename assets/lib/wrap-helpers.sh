#!/bin/bash

wrap_no_hardened_malloc() {
    local bin="$1"
    if [ ! -f "$bin" ]; then
        echo "wrap_no_hardened_malloc: $bin not found" >&2
        return 1
    fi
    [ -f "${bin}.bin" ] && return 0
    mv "$bin" "${bin}.bin"
    cat > "$bin" << EOF
#!/bin/bash
exec env -u LD_PRELOAD "${bin}.bin" "\$@"
EOF
    chmod 755 "$bin"
}
