- **[modules.kdl](../modules.kdl)** — the image author's file. Which
- **`modules/<path>/module.kdl`** — the module author's file, required

## What is not declared

| Path | Effect |
| --- | --- |
| `module.sh` | sourced as the install logic |
| `repo` | sourced once, idempotent via its `REPO_ID` |
| `selinux/*.te` | compiled and installed at priority 200 |
| `files/` | copied verbatim into the image |
| `finalize.sh` | sourced by the finalize phase, in resolved order |
| a file another module `collects` | handed to that module |

## modules.kdl

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    family "fedora"
    provides "rechunking" "initramfs-generation" "mac-policy"
    provides-file "/usr/bin/bootc" "/usr/bin/systemctl" "/usr/bin/rpm-ostree"
}

flavours {
    dev
}

modules {
    module "base"
    module "core/auto-updates"
    module "kernel/cachyos-kernel"

        fonts "JetBrainsMono" "FiraCode"
    }

    flavour "dev" {
        module "dev-tools"
    }

    module "core/power-just-scripts"
}
```

### `base`

| Child | Meaning |
| --- | --- |
| `family` | which distro's packaging and tooling modules may assume. Checked against every enabled module's `supports`. Required. |
| `provides` | capabilities the base satisfies that no module could implement portably. A module may `require` one; nothing has to provide it. |
| `provides-file` | absolute paths to binaries the base guarantees. Checked on the finished image alongside the modules' own [contract files](#contract-files). |

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

| Property | Will mean |
| --- | --- |
| `source=` | repository to fetch the module from |
| `ref=` | exact commit or tag |
| `sha256=` | hash of the fetched archive |

### `flavour`

```kdl
flavour "<name>" {
    module "<path>"
}
```

## module.kdl

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

### Collecting

```kdl
collects "justfile.inc" into="/usr/share/goojust/justfile.apps"

collects "flatpaks.list" into="/usr/share/tectonic/default-flatpaks"
```

| Part | Meaning |
| --- | --- |
| argument | filename in a contributing module's directory |
| `into=` | absolute destination in the image, created if needed |

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
| `none` | `tectonic` | `none` | unset |
| `dev` | `tectonic-dev` | `dev` | `dev` |

## What the layer sees

| Env | When |
| --- | --- |
| `FLAVOUR_GATE=<flavour>` | the entry is inside a `flavour` block |
| `OPT_<NAME>=<value>` | one per declared option, always, defaults included |
| `ASSET_<NAME>_VERSION`, `_URL`, `_SHA256` | one per declared asset field, URL already resolved |
| `MODULE_COLLECT="<file>=<dest> ..."` | this module ships a file another module collects |
| `<NAME>=${<NAME>}` | one per `arg` |

## Validation

- either file unparseable, or carrying a node or property this schema
- a `modules.kdl` entry that does not resolve to a module directory
- a module directory without a `module.kdl`, or one missing
- no `base` node, a `base` declared twice, one with no image reference or
- an enabled module whose `supports` does not include the base `family`

- a flavour name outside `^[a-z][a-z0-9-]*$`, duplicated, or named `none`
- a `flavours` block with no `default=#true`, or with more than one
- more than one `pr-build=#true`
- a `flavour` block naming an undeclared flavour

- a `requires` no enabled module provides, listing every module that
- a `requires-file` no enabled module provides
- two enabled modules providing the same capability or contract file
- a module providing something the `base` node already provides
- a module shipping `selinux/*.te` without `requires "mac-policy"`
- a requirement satisfied only by a module gated to another flavour
- a cycle, naming the edges that close it

- two enabled modules that land in the same image shipping the same
- an `overrides` for a path no earlier module ships

- shipping a collected filename while the module that collects it is not
- two enabled modules collecting the same filename

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

- `source`, `ref` or `sha256` on a list entry

## Not implemented yet

- **`source`, `ref` and `sha256`** on list entries, for out-of-tree
