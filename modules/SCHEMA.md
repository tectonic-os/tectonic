- **[modules.kdl](../modules.kdl)** — the image author's file. Which
- **`modules/<path>/module.kdl`** — the module author's file, required

## What is not declared

| Path | Effect |
| --- | --- |
| `module.sh` | sourced as the install logic |
| `repo` | sourced once, idempotent via its `REPO_ID` |
| `versions.sh` | sourced for Renovate-tracked pins |
| `selinux/*.te` | compiled and installed at priority 200 |
| `files/` | copied verbatim into the image |
| `finalize.sh` | sourced by the finalize phase, in resolved order |
| a file another module `collects` | handed to that module |

## modules.kdl

```kdl
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
| `requires-file "<abs-path>"` | this module reads it, and fails without it |

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

### Build inputs

| Node | Emits |
| --- | --- |
| `secret "<id>"` | `--mount=type=secret,id=<id>,target=/run/secrets/<id>,required=false` |
| `arg "<NAME>"` | `<NAME>=${<NAME>}` in the layer's env prefix |

### Raw fragments

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
| `MODULE_COLLECT="<file>=<dest> ..."` | this module ships a file another module collects |
| `<NAME>=${<NAME>}` | one per `arg` |

## Validation

- either file unparseable, or carrying a node or property this schema
- a `modules.kdl` entry that does not resolve to a module directory
- a module directory without a `module.kdl`, or one missing

- a flavour name outside `^[a-z][a-z0-9-]*$`, duplicated, or named `none`
- a `flavours` block with no `default=#true`, or with more than one
- more than one `pr-build=#true`
- a `flavour` block naming an undeclared flavour

- a `requires` no enabled module provides, listing every module that
- a `requires-file` no enabled module provides
- two enabled modules providing the same capability or contract file

- shipping a collected filename while the module that collects it is not
- two enabled modules collecting the same filename

- setting an option the module does not declare, or setting one twice
- a value that does not match the declared type
- a `list` value containing whitespace
- selecting an undeclared variant, or a variant setting an undeclared

- a module declaring `secret` or `arg` alongside a `Containerfile.inc`
- a `Containerfile.inc` expanding `FLAVOUR` above the `ARG FLAVOUR`

- `source`, `ref` or `sha256` on a list entry

## Not implemented yet

- **Ordering by the graph.** The build order is document order today. A
- **Additive fragments.** `Containerfile.inc` gains a declared position
- **The overlay collision check**, and with it an `overrides` node
- **`asset` blocks replacing `versions.sh`**: datasource, version,
- **`packages { fedora "..." }`**, declaring packages instead of calling
- **`source`, `ref` and `sha256`** on list entries, for out-of-tree
