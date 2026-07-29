case $- in
    *i*)
        __os_cli_name="$(. /etc/os-release 2>/dev/null && printf '%s' "${NAME%% *}" | tr '[:upper:]' '[:lower:]')"
        # shellcheck disable=SC2139
        [ -n "$__os_cli_name" ] && alias "$__os_cli_name"='goojust'
        unset __os_cli_name
        ;;
esac
