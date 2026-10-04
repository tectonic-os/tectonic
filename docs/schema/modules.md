# Modules

A module is a directory under `modules/`, at whatever depth groups it, and its path names the module. The build reads the files below by convention, so a module can be as small as one `module.sh`. A module can also hold other modules. The walk that finds modules passes over the directories that the build of a module reads.

```text
modules/<module-name>/
        ├── <family>/
        ├── apparmor/
        ├── files/
        ├── selinux/
        │   └── *.te
        ├── <collected file>
        ├── Containerfile.inc
        ├── finalize.sh
        ├── module.kdl
        ├── module.sh
        ├── provenance.kdl
        └── repo
```

| Path | Description |
| --- | --- |
| `module.kdl` | What the module declares about itself. A module with only a script and `files/` can leave it out. A module fetched from another repository needs one. See [`module.kdl`](module.md). |
| `module.sh` | The install logic. The build sources it in the layer of the module. |
| `finalize.sh` | The finalize phase sources it, in resolved order. |
| `files/` | An overlay that the build copies over `/`. |
| `repo` | A package repository file. The build sources it first. On Fedora, if the `REPO_ID` of `repo` is already configured, then the build skips it. |
| `Containerfile.inc` | Containerfile lines that the build adds verbatim, for a need that the fields cannot express. `fragment` in `module.kdl` places them. |
| `selinux/*.te` | SELinux policy. If the image provides `selinux-policy`, then the build compiles and installs it. |
| `apparmor/` | AppArmor profiles. If the image provides `apparmor-policy`, then the build validates them and places them in `/etc/apparmor.d`. |
| `<family>/` | The files of the module that apply to one base family. |
| `<collected file>` | A part of a file that another module collects. The build stages it for that module. |
| `provenance.kdl` | Where a copied module came from, which `tect copy module` writes. See [`provenance.kdl`](provenance.md). |

A policy directory follows these rules:

- A module that ships `selinux/` or `apparmor/` builds after the module that provides that MAC, as if it declared `after`.
- A module can ship both policy directories. The build takes each directory only where the image has that MAC. An image with no MAC installs no policy and needs no provider.
- A policy directory declares no requirement. A module that needs a MAC declares `requires "selinux-policy"` or `requires "apparmor-policy"`, and `tect` refuses it on an image without that MAC.

A `<family>/` directory gates `module.sh`, `finalize.sh`, `files/`, `repo` and a collected file:

- The six directory names are `fedora/`, `rhel/`, `debian/`, `ubuntu/`, `rpm/` and `deb/`. The build reads no other directory in a module.
- The build takes `deb/` on Debian and Ubuntu, and `rpm/` on Fedora and RHEL. It takes `debian/` on Debian only, after `deb/`.
- A gated half runs after the ungated half. The family `files/` is copied over the shared `files/`, and the family `module.sh` is sourced after the shared `module.sh`.
- A family `repo` replaces the other copies. `debian/repo` wins over `deb/repo`, which wins over the `repo` at the module root, and the build sources only the winner.
- If `supports` is declared and the directory names no supported family, then `tect` refuses it. With no `supports` declaration, every family is supported.

> [!NOTE]
> A module with no `<family>/` directory gates nothing. Its `module.sh` and `files/` run on every family it supports; with no `supports` declaration, that is every family.
