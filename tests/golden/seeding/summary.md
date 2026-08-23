3 modules, the ungated set.

| Module | Description | Options | Satisfies |
| --- | --- | --- | --- |
| `fedora-family` | dnf5 and its plugins, which every fedora build layer reaches for |  |  |
| `hello` | A module this repository writes and publishes |  |  |
| `tectonic-os/flatpak` `variant=beta` `remote=v1.0.0` | A module imported from the collection this repository publishes | `OPT_REMOTES="flathub flathub-beta"` `OPT_SYSTEM_WIDE="0"` |  |
