# Module manifests

`module.kdl` is the module's whole interface: what it builds on, what it needs
from the rest of the image, and what an image author may set. Everything else
in the directory is convention.

| Path | What the build does with it |
| --- | --- |
| `repo` | sourced first; on Fedora, unless its `REPO_ID` is already configured |
| `module.sh` | sourced as the install logic |
| `selinux/*.te` | compiled and installed, where the image provides `selinux-policy` |
| `apparmor/*` | validated and placed in `/etc/apparmor.d`, where the image provides `apparmor-policy` |
| `files/` | copied over `/` |
| `finalize.sh` | sourced by the finalize phase, in resolved order |
| `Containerfile.inc` | placed verbatim by `fragment`, with `@MODULE@` replaced by the module's directory in the build context |
| `<family>/` | `repo`, `module.sh`, `files/`, `finalize.sh` and a collected file under `fedora/`, `rhel/`, `debian/`, `ubuntu/`, `rpm/` or `deb/`, each taken on the families that name is for |
| a file another module collects | staged for it |

A fragment is inlined verbatim and so cannot name its own directory. `@MODULE@`
expands to the path the module layer's mounts use, `/modules/<dir>`, which is
what a `COPY --from=ctx` needs to reach a file the module ships — the way to
write `/etc/hostname`, `/etc/hosts` or `/etc/resolv.conf`, which are bind-mounted
over during every `RUN`.

Shipping either policy directory also orders the module: it builds after
whoever provides that MAC, the way an `after` would, without declaring one. A
module that ships for both is built for whichever the image carries, so it
cannot name one — and an image with no MAC at all installs no policy and so
owes no provider.

```kdl
description "kvmfr DKMS module for GPU passthrough"

supports "fedora"

requires "kernel-devel"
after "vfio"

secret "mok_privkey"
arg "KERNEL"
```

A module may ship both, and each is taken only where the image has that MAC,
the way a `packages` batch is taken for the base's family. Neither directory
declares anything on its own: a module that cannot work without a given MAC
says so with `requires "selinux-policy"` or `requires "apparmor-policy"`, and
is refused on an image that has not got it like any other requirement.

Build order is resolved from `requires` and `after`, never from the order of
the list, which is only a tie-break. A `requires` nothing provides fails the
check and names every module that would satisfy it, and so does an `after`
nothing provides — both are edges, and an edge to nothing is a declaration
about a module that is not there. A `family` gate is how an ordering true on
one family alone is written without dangling on the others. A capability name is lowercase letters, digits and
dashes, starting with a letter, since the generated graph writes it into a
mermaid label and a markdown table cell without quoting it.

### Gating by family

A module that builds on more than one base family usually installs different
package names on each, and sometimes does something different altogether. Two
things gate on the family, one for what the manifest declares and one for what
the directory ships. Both follow the same rule: **outside a gate is every
family the module supports, inside a gate is the families it names.**

`family` gates declarations. It takes one or more family names, so a list two
families share is written once rather than twice:

```kdl
description "Traditional CLI utilities"

supports "fedora" "debian" "ubuntu"

packages "bash-completion" "htop" "rsync" "tmux"

family "fedora" {
    packages "7zip-standalone" "vim-enhanced"
    copr "someone/tools"
}

family "debian" "ubuntu" {
    packages "7zip" "vim"
}
```

`packages`, `package-groups`, `copr`, `requires`, `after` and `satisfies` go
inside one. The rest of the manifest does not: `description` and `supports` are
what the module is and what it claims, `option`, `variant` and `asset` are the
image author's interface and must not change shape underneath them, and `key`,
`collects`, `contributes` and `helpers` are contracts with other modules.
`provides` is left out too — a capability offered on one family and not another
is a module that should have been two.

A gate is read whether or not the image is built for it, so a manifest is held
to the same checks on every family it claims rather than only on the one being
built. What a gate the build does not want declared is then dropped: a
Fedora-only `after` does not dangle on a Debian image, and a Fedora rule number
in `satisfies` is not a claim a Debian scan can fail to map.

`<family>/` gates files, because `module.sh`, `finalize.sh` and `files/` are
files and no node names them. A `debian/` directory holding any of the three is
taken on Debian alone, the way `selinux/` is taken only where the image has
that MAC:

```
modules/login-access/
├── module.kdl
├── files/            # every family
├── module.sh         # every family
└── debian/
    ├── files/        # Debian alone, copied after files/
    └── module.sh     # Debian alone, sourced after module.sh
```

`fedora/`, `rhel/`, `debian/`, `ubuntu/`, `rpm/` and `deb/` are the six names;
anything else in a module directory is the author's own and is read by
nothing.

A file another module collects gates too, and is one of the two cases that
**picks** rather than layers: a collector claims one filename across the image,
so a module ships at most one copy of it and the most specific directory
holding one wins — `debian/justfile.inc`, else `deb/justfile.inc`, else the one
at the root. `contributes` names the file once and says nothing about where it
came from.

`repo` is the other. An archive is configured once and the two families name
it at different URLs rather than adding to one another, so `debian/repo` wins
over `deb/repo` wins over the one at the root, and only the winner is
sourced.

**The convention is additive and nothing has to be renamed to use it.** An
ungated `module.sh` and `files/` at the module root mean exactly what they
always meant: run everywhere. A module with no `<family>/` directory has
nothing gated, which is the overwhelmingly common case, and a module written
for one base family writes no gate at all — `packages "htop"` under
`supports "fedora"` installs on Fedora because that is the only family it
supports. The convention earns its keep in a collection published for
consumers whose base its author does not control, where the alternative is
duplicating every module per family or refusing the family outright.

The gated half runs after the ungated one rather than instead of it: the
family `files/` is copied over the shared one, so it may replace a shared file,
and the family `module.sh` is sourced below the shared one. A `<family>/`
directory or a `family` gate naming a family the module does not `supports` is
refused, since no image could reach it.

A directory name is one family where a `family` gate takes a list, so `deb/`
and `rpm/` are the names for the two families each shares a package manager and
an installer with, which would otherwise hold two copies of one file set. `deb/`
is taken on Debian and Ubuntu both, `rpm/` on Fedora and RHEL, and a `debian/`
beside it is Debian alone and is taken after it — widest first, the same order
as the ungated half and a gate:

```
modules/login-access/
├── files/            # every family
├── deb/files/        # Debian and Ubuntu
└── debian/files/     # Debian alone, copied after deb/files/
```

`collects` claims a filename across the whole image and `contributes` says
this module ships one. Each contribution is staged as
`<into>.d/NNNN-<module>.part` and the finalize phase assembles them in that
order, so what the assembled file looks like does not depend on when its
contributors built.

`satisfies` is a claim, not a measurement. A module knows what it hardens, so
the claim belongs at the module that makes it true; the scan can only confirm
it after the fact.

```kdl
satisfies {
    cis-fedora "1.1.1.1" "5.2.20"
    stig "RHEL-09-232010"
}
```

The node name is the benchmark and the strings are its rule numbers. The
benchmark set is open, because CIS, STIG and whatever a downstream standard is
called are not a set this tool can close.

`generate` writes every claim into `generated/plan.json`, and the compliance
job in `.github/workflows/build.yml` reads it back, resolves each number to an
XCCDF rule id through the SSG datastream, scans the pushed image and compares.
Three things it distinguishes: a number that maps to no rule is a failure of
the **declaration**; a rule the image fails is a **false claim**; and a rule
the image fails where another module's overlay owns the final copy of a file
this one ships is a **composition** that defeats a claim that was honest. The
last is why `plan.json` carries `overlay_overridden`.

A target whose modules declare nothing is not scanned and says so. Only
`.modules[]` is read, never `.suppressed[]`: a module the base displaced
contributes no layer, so its claims are about an image this is not.

<!-- schema: module -->

Also holds [`option`](#option), [`variant`](#variant) and [`asset`](pins.md#asset).

| Node | Takes | Meaning |
| --- | --- | --- |
| `description` | a string, at most one | One line naming the module in the resolved build summary. |
| `supports` | one or more strings | The base families this module builds on, matched against the image's `family`. |
| `requires` | one or more strings | A capability another module has to provide, which also orders the build. |
| `after` | one or more strings | A module this one builds after without requiring anything of it. |
| `overrides` | one or more strings | An absolute path this module replaces deliberately. |
| `mode` | two strings: path, then octal mode, one per name | An octal file mode applied to one path in this module's overlay. |
| `secret` | one or more strings | A build secret this module's layer mounts. |
| `arg` | one or more strings | A build argument this module's layer reads. |
| `helpers` | one or more strings | Files from this module mounted by basename into /ctx/lib in every module layer. |
| `copr` | a string, one per name | A COPR repository this module enables for its own installs, as owner/project. Fedora only. |

### `provides`

A capability this module satisfies for the modules that require it.

*one or more strings*

| Property | Value | Meaning |
| --- | --- | --- |
| `file=` | a string | The absolute path that witnesses the one name given, where it is neither `/usr/bin/<name>` nor `/usr/sbin/<name>`; the finished image is checked for it. |
| `build-only=` | `#true` or `#false` | Whether the `file` exists only while the build runs. |

### `key`

A key `tect create key` generates for this module, and where each half of it goes.

*a string, one per name*

| Node | Takes | Meaning |
| --- | --- | --- |
| `private` | a string, exactly one | What the private half is called under `keys/private/`. |

#### `generator`

Which of the generators the tool implements writes this key.

*`cosign`, `openssl`, `ssh-keygen`, exactly one*

| Property | Value | Meaning |
| --- | --- | --- |
| `profile=` | `module-signing`, `pcr-signing`, `tls-ca` | What the generator is set up for, where it can do more than one thing. |
| `bits=` | 2048 to 16384 | The RSA key size, 4096 where none is named. |

#### `public`

Where the public half is shipped, which witnesses the `<kind>-key` capability this module provides.

*a string, exactly one*

| Property | Value | Meaning |
| --- | --- | --- |
| `format=` | `pem`, `der` | What the public half is written as, PEM where none is named. |

### `allow-verify`

One `tect validate-image` diagnostic accepted on one unit, leaving the rest of the image checked.

*a string*

| Property | Value | Meaning |
| --- | --- | --- |
| `unit=` | a string | The unit the exception applies to. |

### `refuses`

One benchmark rule this module deliberately leaves unsatisfied, which no remediation may set on its behalf.

*a string, one per name*

| Property | Value | Meaning |
| --- | --- | --- |
| `because=` | a string | Why the rule is left unsatisfied, which is the whole point of declaring it. |

### `collects`

A filename this module gathers from every module that ships one.

*a string*

| Property | Value | Meaning |
| --- | --- | --- |
| `into=` | a string | The absolute path the assembled file is written to. |
| `priority=` | 0 to 9999 | Where a contribution lands when it declares none. |

### `contributes`

A file this module ships for another module to collect.

*a string*

| Property | Value | Meaning |
| --- | --- | --- |
| `priority=` | 0 to 9999 | Where this file lands in the assembled one. |

### `fragment`

Where the module's Containerfile.inc goes relative to the generated layer.

*at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `position=` | `before`, `after`, `tail` | Whether the fragment goes above the generated block, below it, or below the finalize layer. |
| `standard-layer=` | `#true` or `#false` | Whether the generated block is emitted at all. |

### `packages`

The packages this module installs, on every family it supports or, inside a `family` block, on the families that names.

*one or more strings*

| Property | Value | Meaning |
| --- | --- | --- |
| `enablerepo=` | a string | A repository enabled for this install and disabled otherwise. dnf only, so the batch has to resolve to `fedora` or `rhel` alone. |

### `package-groups`

The package groups this module installs. dnf only, so an ungated one is a module supporting `fedora` or `rhel` alone.

*one or more strings*

| Property | Value | Meaning |
| --- | --- | --- |
| `enablerepo=` | a string | A repository enabled for this install and disabled otherwise. |

### `satisfies`

The benchmarks and rules this module claims to harden, as an audit declaration. The tool records it and certifies nothing.

*at most one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `<name>` | one or more strings, one per name | One benchmark, and the rule IDs it covers. |

### `family`

The declarations inside taken only on the base families named, everything outside a gate being taken on every family the module supports.

*one or more strings, never empty*

Also holds `packages`, as above, `package-groups`, as above and `satisfies`, as above.

| Node | Takes | Meaning |
| --- | --- | --- |
| `copr` | a string, one per name | A COPR repository this module enables for its own installs, as owner/project. Fedora only. |
| `requires` | one or more strings | A capability another module has to provide, which also orders the build. |
| `after` | one or more strings | A module this one builds after without requiring anything of it. |

<!-- /schema: module -->

## Options

An option is one value an image may set on the module that declared it. Every
declared option reaches that module's layer as `OPT_<NAME>`, uppercased with
dashes as underscores, always, defaults included, so `module.sh` reads a
variable rather than testing whether one is set. A `list` arrives as a bash
array, a `string` or a `bool` as a scalar.

```kdl
option "fonts" type="list" {
    description "Nerd Font families to install"
    default "JetBrainsMono" "FiraCode"
}

option "starship" type="bool" {
    description "Install the starship prompt"
    default #true
}
```

| `type=` | KDL value | Env value |
| --- | --- | --- |
| `string` | `"text"` | verbatim |
| `bool` | `#true` or `#false` | `1` or `0` |
| `list` | zero or more strings | space joined |

<!-- schema: option -->

### `option`

One value an image may set on this module, reaching the build as OPT_*.

*a string, one per name*

| Property | Value | Meaning |
| --- | --- | --- |
| `type=` | a string | What the option holds: string, bool or list. |

| Node | Takes | Meaning |
| --- | --- | --- |
| `description` | a string, at most one | What setting the option does, for the generated reference. |
| `default` | one or more strings, at most one | What the module builds with when no image sets it. |

<!-- /schema: option -->

## Variants

A variant is a named set of option values, so an image can take a whole
position on a module with one word instead of setting five options
consistently.

```kdl
variant "wine-only" {
    description "Skip the metadata and .NET payloads"
    set "dotnet" #false
    set "winmd" #false
}
```

An image selects one with `variant="wine-only"` on its list entry, and may
still set an option itself, which wins.

<!-- schema: variant -->

### `variant`

A named set of option values an image selects with `variant=`.

*a string, one per name*

| Node | Takes | Meaning |
| --- | --- | --- |
| `description` | a string, at most one | What the variant is for. |
| `set` | a string | One option this variant sets, and what it sets it to. |

<!-- /schema: variant -->
