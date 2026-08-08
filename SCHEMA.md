| File | Declares |
| --- | --- |
| `<name>.kdl` at the root | One image: its name, its base, its flavours, and the modules in it |
| `repo.kdl` | The repository: the schema it is written against, which image a bare build builds, and which CI workflows run |
| `modules/<path>/module.kdl` | One module: what it needs, what it offers, and what an image author may configure |

## The image files

```kdl
image {
    name "Tectonic"
    url "https://github.com/tectonic-os/tectonic"
    issues-url "https://github.com/tectonic-os/tectonic/issues"

    base "quay.io/fedora/fedora-bootc:44" {
        family "fedora"
        provides "rechunking" "initramfs-generation" "mac-policy"
        provides-file "/usr/bin/bootc" "/usr/bin/systemctl" "/usr/bin/rpm-ostree"
    }

    flavours {
        dev
    }

    modules {
        module "core/bootloader"
        module "core/auto-updates"
        module "kernel/cachyos-kernel"

            fonts "JetBrainsMono" "FiraCode"
        }

        flavour "dev" {
            module "dev-tools"
        }

        module "core/power-just-scripts"
    }
}
```

```kdl
image {
    name "Tectonic Server"
    id "tectonic-server"

    base "quay.io/centos-bootc/centos-bootc:stream10" {
        family "centos"
        provides "rechunking" "initramfs-generation" "mac-policy"
        provides-file "/usr/bin/bootc" "/usr/bin/systemctl"
    }

    modules {
        module "core/bootloader"
        module "core/signature-policy"
        module "hardening/login-policy"
        module "virtualization/podman"
    }
}
```

```console
$ manifest plan --json | jq -r '.images[].targets[].name'
tectonic/none
tectonic/dev
tectonic-server/none
```

### The three names

| | where it comes from | where it goes |
| --- | --- | --- |
| the file name | you | nowhere. Not the build, not the artifact |
| `id` | `name` lowercased, or declared | published image name, cache tag, build target, os-release `DEFAULT_HOSTNAME`, MOK key directory |
| `name` | declared | os-release `NAME`, and `PRETTY_NAME` through its default |

### `image`

| Child | Meaning |
| --- | --- |
| `name` | os-release `NAME`, the human one. Required. |
| `id` | the machine name, matching `^[a-z][a-z0-9-]*$` because it becomes an image tag, a cache tag and the default hostname. Derived from `name` when absent. |
| `pretty-name` | os-release `PRETTY_NAME`. Defaults to `<name> <version>`. |
| `url` | os-release `HOME_URL` and `DOCUMENTATION_URL`. |
| `issues-url` | os-release `SUPPORT_URL` and `BUG_REPORT_URL`. |

### `base`

| Child | Meaning |
| --- | --- |
| `family` | which distro's packaging and tooling modules may assume. Checked against every enabled module's `supports`. Required. |
| `provides` | capabilities the base satisfies that no module could implement portably. A module may `require` one; nothing has to provide it. |
| `provides-file` | absolute paths to binaries the base guarantees. Checked on the finished image alongside the modules' own [contract files](#contract-files). |
| `signed` | whether the base image publishes a cosign signature. Optional, `#false` when absent. |

### `flavours`

| Property | Meaning |
| --- | --- |
| `default=#true` | the flavour built when none is named. Exactly one, required when the block is present. |
| `pr-build=#true` | the single flavour a pull request builds. At most one; falls back to the default. |

### `module`

```kdl
module "<path>" variant="<name>" {
    <option-name> <value...>
}
```

| Property | Meaning |
| --- | --- |
| `variant=` | selects a `variant` block declared in the module's own manifest |

```kdl
    fonts "JetBrainsMono" "FiraCode"
    starship #true
}
```

### Out-of-tree modules

```kdl
module "steam-tweaks" {
    source "https://github.com/owner/bootc-modules/archive/refs/tags/{ref}.tar.gz" {
        renovate datasource="github-tags" depName="owner/bootc-modules"
        ref "steam-tweaks/v1.2.0"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
        path "modules/steam-tweaks"
    }
}
```

| Node | Arity | Meaning |
| --- | --- | --- |
| `source "<template>"` | 0 or 1 | the archive to fetch. `{ref}` is the only expansion. |
| `renovate` | 0 or 1 | Renovate tracks this pin. Mutually exclusive with `manual`. |
| `manual "<why>"` | 0 or 1 | nothing tracks it, and this is why. Mutually exclusive with `renovate`. |
| `ref "<pin>"` | exactly 1 | the exact tag or commit the URL resolves against |
| `sha256 "<hex>"` | exactly 1 | what the fetched archive must hash to |
| `path "<subtree>"` | 0 or 1 | the module's directory inside the archive. Absent means the archive root is the module. |

- the generator emits the same RUN block as for an in-tree module, so
- a remote module ships the same required `module.kdl` and is validated
- **no transitive fetching.** A remote module may `requires` a

### `flavour`

```kdl
flavour "<name>" {
    module "<path>"
}
```

## repo.kdl

```kdl
schema-version 1

default-image "tectonic"
pr-image "tectonic"

workflows {
    smoke-test enabled=#false
}
```

### `schema-version`

### `default-image` and `pr-image`

### `workflows`

| Property | Meaning |
| --- | --- |
| `enabled=#false` | the workflow does not run |
| `enabled=#true` | the workflow runs, which is also what silence means |

## module.kdl

| Path | Effect |
| --- | --- |
| `module.sh` | sourced as the install logic |
| `repo` | sourced once, idempotent via its `REPO_ID` |
| `selinux/*.te` | compiled and installed at priority 200 |
| `files/` | copied verbatim into the image |
| `finalize.sh` | sourced by the finalize phase, in resolved order |
| a file another module `collects` | handed to that module |

```kdl
description "kvmfr DKMS module for Looking Glass GPU passthrough"

supports "fedora"

requires "kernel-devel"
after "vfio"

secret "mok_privkey"
arg "KERNEL"
```

### Identity

| Node | Arity | Meaning |
| --- | --- | --- |
| `description "..."` | exactly 1 | one line, present tense, no trailing period. Shown in the resolved build summary. |
| `supports "<family>"` | 1+ | base families this module can build on. `fedora` is the only one today. |

### Capabilities

| Node | Kind | Unsatisfied |
| --- | --- | --- |
| `provides "<cap>"` | — | — |
| `requires "<cap>"` | hard | error, naming every module that would satisfy it |
| `after "<cap>"` | soft | ignored |

### Contract files

| Node | Meaning |
| --- | --- |
| `provides-file "<abs-path>"` | this module writes it |
| `provides-file "<abs-path>" build-only=#true` | it writes it for other build layers, then removes it again |
| `requires-file "<abs-path>"` | this module reads it, and fails without it |

### Overlay collisions

| Node | Meaning |
| --- | --- |
| `overrides "<abs-path>"` | this module's overlay knowingly replaces a path an earlier module ships |

```console
$ manifest plan --json | jq -r '
    .images[].targets[] | select(.name == "tectonic/dev")
    | .overlay_files["/usr/lib/modprobe.d/vfio.conf"]'
virtualization/vfio-passthrough
```

### Verify exceptions

| Node | Meaning |
| --- | --- |
| `allow-verify "<class>" unit="<unit>"` | this diagnostic class is expected on this unit |

| Class | What it is |
| --- | --- |
| `mount-not-found` | a unit ordered against a `.mount` or `.swap` unit, which a container build has not got |
| `man-page-missing` | a `Documentation=` man page this image does not carry, which verify checks by running `man` against it |

```
FAIL: tuned.service: systemd-analyze verify
      tuned.service: Command 'man tuned(8)' failed with code 16
        this is the known class 'man-page-missing'. If it is expected here,
        declare it in the module shipping 45-module-kde-desktop.preset:
          allow-verify "man-page-missing" unit="tuned.service"
```

### Collecting

```kdl
collects "justfile.inc" into="/usr/share/goojust/justfile.apps" priority=500

collects "flatpaks.list" into="/usr/share/flatpak-defaults/apps.list" priority=500
```

| Part | Meaning |
| --- | --- |
| argument | filename in a contributing module's directory |
| `into=` | absolute destination in the image, created if needed |
| `priority=` | 0 to 9999, where a contribution that names none lands |

```kdl
contributes "justfile.inc" priority=900
```

### Options

```kdl
option "fonts" type="list" {
    description "Nerd Font families to install"
    default "JetBrainsMono" "FiraCode" "CascadiaMono"
}

option "starship" type="bool" {
    description "Install the starship prompt"
    default #true
}
```

| Type | KDL value | Env value |
| --- | --- | --- |
| `string` | `"text"` | verbatim |
| `bool` | `#true` / `#false` | `1` / `0` |
| `list` | zero or more strings | space joined |

### Variants

```kdl
variant "wine-only" {
    description "Skip the WinRT metadata and .NET payloads"
    set "dotnet" #false
    set "winmd" #false
}
```

### Asset pins

```kdl
asset "starship" {
    renovate datasource="github-releases" depName="starship/starship"
    version "1.26.0"
    url "https://github.com/starship/starship/releases/download/v{version}/starship-x86_64-unknown-linux-musl.tar.gz"
    sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3" from="sidecar"
}
```

| Node | Arity | Meaning |
| --- | --- | --- |
| `renovate` | 0 or 1 | Renovate tracks this pin. Mutually exclusive with `manual`. |
| `manual "<why>"` | 0 or 1 | nothing tracks it, and this is why. Mutually exclusive with `renovate`. |
| `version "<pin>"` | 0 or 1 | the pinned ref: a version, a tag or a commit. Required with `renovate`. |
| `url "<template>"` | 0 or 1 | download URL. `{version}` is the only expansion. |
| `sha256 "<hex>"` | 0 or 1 | what the fetched bytes must hash to. |

| `from=` | Where the hash comes from |
| --- | --- |
| `"asset"` (default) | hashing the asset itself. Trust-on-first-use, taken at PR time, which still catches an asset swapped after the pin was made. |
| `"sidecar"` | the `<url>.sha256` upstream publishes beside it, so the pin is accurate from the start. |
| `"manual"` | a human. For an asset whose filename does not follow from its version, or that has no version at all. |

#### What Renovate reads

| Property | Meaning |
| --- | --- |
| `datasource=` | `github-releases`, `github-tags` or `git-refs` — the three the custom managers match |
| `depName=` | `owner/repo`, or the clone URL for `git-refs` |
| `extractVersion=` | Renovate's capture turning an upstream tag into the value pinned here, e.g. `^v(?<version>.*)$` |

### Packages

```kdl
packages {
    fedora "just" "fastfetch"
    fedora "tailscale" enablerepo="tailscale-stable"
}
```

| Property | Meaning |
| --- | --- |
| `enablerepo=` | install from a repo the base image already carries disabled. Not the module's own `repo` file: that is sourced by `run-module.sh`, after the generated install runs. |

### Build inputs

| Node | Emits |
| --- | --- |
| `secret "<id>"` | `--mount=type=secret,id=<id>,target=/run/secrets/<id>,required=false` |
| `arg "<NAME>"` | `<NAME>=${<NAME>}` in the layer's env prefix |

### Raw fragments

```kdl
fragment position="after" standard-layer=#false
```

| Property | Default | Meaning |
| --- | --- | --- |
| `position=` | `"before"` | where the fragment goes relative to the generated block |
| `standard-layer=` | `#true` | whether that block is emitted at all |

## Build order

## Build targets

| Target | Image | Cache tag | `FLAVOUR` |
| --- | --- | --- | --- |
| `tectonic/none` | `tectonic` | `tectonic` | unset |
| `tectonic/dev` | `tectonic-dev` | `tectonic-dev` | `dev` |
| `tectonic-server/none` | `tectonic-server` | `tectonic-server` | unset |

## What the layer sees

| Env | When |
| --- | --- |
| `FLAVOUR_GATE=<flavour>` | the entry is inside a `flavour` block |
| `OPT_<NAME>=<value>` | one per declared option, always, defaults included |
| `ASSET_<NAME>_VERSION`, `_URL`, `_SHA256` | one per declared asset field, URL already resolved |
| `MODULE_COLLECT="<file>=<staged path> ..."` | this module ships a file another module collects |
| `<NAME>=${<NAME>}` | one per `arg` |

## Validation

- any file unparseable, or carrying a node or property this schema
- a module directory without a `module.kdl`, or one missing
- a repository declaring no image, or a root `.kdl` that declares none
- a `repo.kdl` with no `schema-version`, or one this tool does not know
- an `image` node with an argument, no `name` child, an `id` outside
- two images declaring the same `id`, or two builds that would publish
- `default-image` missing with more than one image declared, or either it
- an image with no `base`, a `base` declared twice, one with no image
- an enabled module whose `supports` does not include the base `family`

- a flavour name outside `^[a-z][a-z0-9-]*$`, duplicated, or named `none`
- a `flavours` block with no `default=#true`, or with more than one
- more than one `pr-build=#true`
- a `flavour` block naming an undeclared flavour

- a `workflows` entry naming no file under `.github/workflows/`, listing
- a workflow declared twice, or one with no `enabled`
- an empty `workflows` block

- a `requires` no enabled module provides, listing every module that
- a `requires-file` no enabled module provides
- two enabled modules providing the same capability or contract file
- a module providing something the `base` node already provides
- a module shipping `selinux/*.te` without `requires "mac-policy"`
- a requirement satisfied only by a module gated to another flavour
- a cycle, naming the edges that close it

- two enabled modules that land in the same image shipping the same
- an `overrides` for a path no earlier module ships

- an `allow-verify` naming a class outside the known set, listing them
- an `allow-verify` with no class, or no `unit=`
- the same class allowed twice on the same unit in one module

- shipping a collected filename while the module that collects it is not
- two enabled modules collecting the same filename
- a `collects` with no `into=`, no `priority=`, or a relative `into=`
- a `contributes` for a filename the module does not ship
- the same filename ordered twice in one module
- a `priority` outside 0 to 9999

- an asset declaring neither `renovate` nor `manual`, or both
- a `renovate` with no `depName`, or a datasource no custom manager
- a `renovate` with no `version` below it, or with something between the
- a `manual` with no reason
- a `url` without a `sha256`, or a `sha256` without a `url`
- a `sha256` that is not 64 lowercase hex digits
- a `url` holding a placeholder other than `{version}`, or holding
- two assets in one module under the same name
- a `version` or `url` containing a shell metacharacter, which the env

- setting an option the module does not declare, or setting one twice
- a value that does not match the declared type
- a `list` value containing whitespace
- selecting an undeclared variant, or a variant setting an undeclared

- a `fragment` node in a module that ships no `Containerfile.inc`, or
- a `position` other than `before` or `after`, or one declared alongside
- a `secret`, `arg`, `option`, `asset` or collected file declared
- a `Containerfile.inc` expanding `FLAVOUR` above the `ARG FLAVOUR`
- a gated module whose fragment runs a command without carrying the

- a pin declaring neither `renovate` nor `manual`, or both
- a `renovate` with something between it and the `ref` below it, or
- a pin with no `ref`, or no `sha256`
- a `sha256` that is not 64 lowercase hex digits
- a source URL that is not https or file, is not a tar archive, holds a
- a subtree `path` that is absolute or holds a `..` segment
- a pinned name that is not one lowercase path segment, or that is also
- a URL, ref or path holding a character the fetch could not carry
- an option named `source`, which a list entry's pin already claims

## Not implemented yet

- **A variant overriding an asset pin.** A pin is not an option, and no
