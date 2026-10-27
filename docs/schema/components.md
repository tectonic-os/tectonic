# Components

A component is a directory under `components/`, at whatever depth groups it, and its path names the component. The build reads the files below by convention, so a component can be as small as one `component.sh`. Any one of `manifest.kdl`, `component.sh`, `repo.sh`, `packages.sh`, `finalize.sh`, `Containerfile.inc` and `files/` marks a directory as a component. A component can also hold other components. An image that lists a nested component also gets the nearest component that holds it, and an image that lists the outer component gets none of the components inside it. The walk that finds components passes over the directories that the build of a component reads.

```text
components/<component-name>/
           ├── <family>/
           ├── apparmor/
           ├── files/
           ├── selinux/
           │   └── *.te
           ├── <collected file>
           ├── Containerfile.inc
           ├── component.sh
           ├── finalize.sh
           ├── manifest.kdl
           ├── packages.sh
           └── repo.sh
```

| Path | Description |
| --- | --- |
| `manifest.kdl` | What the component declares about itself. A component with only a script and `files/` can leave it out. A component fetched from another repository needs one. See [`manifest.kdl`](component.md). |
| `component.sh` | The install logic. The build sources it in the layer of the component, after the keys and before the policies and `files/`. |
| `packages.sh` | Package work that the declarations cannot express. The package step sources it after the declared packages, with the network that `packages` in `repo.kdl` allows. It can call the family's `install_packages`. |
| `finalize.sh` | The finalize phase sources it, in resolved order. |
| `files/` | An overlay that the build copies over `/`. |
| `repo.sh` | A package repository file. The build sources it first. On Fedora, if the `REPO_ID` of `repo.sh` is already configured, then the build skips it. |
| `Containerfile.inc` | Containerfile lines that the build adds verbatim, for a need that the fields cannot express. `fragment` in `manifest.kdl` places them. |
| `selinux/*.te` | SELinux policy. If the image provides `selinux-policy`, then the build compiles and installs it. |
| `apparmor/` | AppArmor profiles. If the image provides `apparmor-policy`, then the build validates them and places them in `/etc/apparmor.d`. |
| `<family>/` | The files of the component that apply to one base family. |
| `<collected file>` | A part of a file that another component collects. The build stages it for that component. |

A policy directory follows these rules:

- A component that ships `selinux/` or `apparmor/` builds after the component that provides that MAC, as if it declared `after`.
- A component can ship both policy directories. The build takes each directory only where the image has that MAC. An image with no MAC installs no policy and needs no provider.
- A policy directory declares no requirement. A component that needs a MAC declares `requires "selinux-policy"` or `requires "apparmor-policy"`, and `tect` refuses it on an image without that MAC.

A `<family>/` directory gates `component.sh`, `packages.sh`, `finalize.sh`, `files/`, `repo.sh` and a collected file:

- The six directory names are `fedora/`, `rhel/`, `debian/`, `ubuntu/`, `rpm/` and `deb/`. The build reads no other directory in a component.
- The build takes `deb/` on Debian and Ubuntu, and `rpm/` on Fedora and RHEL. It takes `debian/` on Debian only, after `deb/`.
- A gated half runs after the ungated half. The family `files/` is copied over the shared `files/`. The family `component.sh`, `packages.sh` and `finalize.sh` are each sourced after the shared copy, from `deb/` or `rpm/` to the single family.
- A family `repo.sh` replaces the other copies. `debian/repo.sh` wins over `deb/repo.sh`, which wins over the `repo.sh` at the component root, and the build sources only the winner.
- If `targets` is declared and the directory names no targeted family, then `tect` refuses it. With no `targets` declaration, every family is targeted.

> [!NOTE]
> A component with no `<family>/` directory gates nothing. Its `component.sh` and `files/` run on every family it targets; with no `targets` declaration, that is every family.
