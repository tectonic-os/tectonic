#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

target="${1:?usage: summary.sh <target>}"

./scripts/manifest.sh plan --json | jq -r --arg target "$target" '
    def cell: gsub("\\|"; "\\\\|");

    .images[].targets[] | select(.name == $target)
    | (if .flavour == null
       then "\(.modules | length) modules, the ungated set."
       else "\(.modules | length) modules, \([.modules[] | select(.flavour)] | length) of them gated to `\(.flavour)`."
       end),
      "",
      "| Module | Description | Options |",
      "| --- | --- | --- |",
      (.modules[]
        | "| `\(.path)`"
        + (if .flavour then " `[\(.flavour)]`" else "" end)
        + (if .variant then " `variant=\(.variant)`" else "" end)
        + (if .remote then " `remote=\(.remote)`" else "" end)
        + " | \(.description | cell)"
        + " | \([.options | to_entries[] | "`\(.key)=\"\(.value | cell)\"`"] | join(" "))"
        + " |")
'
