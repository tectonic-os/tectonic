# The schema

Three kinds of file, all KDL, all read by `tect`.

| File | Declares |
| --- | --- |
| `repo.kdl` | the repository: the schema it is written against, which image a bare build builds, which shipped workflows run |
| `<name>.kdl` at the root | one image: what it calls itself, what it builds on, and the modules in it |
| `modules/<path>/module.kdl` | one module: what it needs, what it offers, and what an image author may set |

`tect check` reads all three and reports every problem at the line that caused
it. The reference under each heading below is generated from the tables the
parser walks, so it cannot drift from what the tool accepts.

## The repository file

`repo.kdl` holds what is true of the repository rather than of any image in
it.

```kdl
schema-version 1

default-image "workstation"
pr-image "workstation"

workflows {
    smoke-test enabled=#false
}
```

`schema-version` picks the reader, so a repository written against an earlier
release keeps building: a tool that does not know the version says so plainly
instead of reporting every node it cannot place.

A workflow is named by its file stem under `.github/workflows/`, and one
nobody names runs. The block is how a repository turns something off.

<!-- schema: repo -->

| Node | Takes | Meaning |
| --- | --- | --- |
| `schema-version` | a number, at most one | The schema release this repository is written against, which picks the reader. |
| `default-image` | a string, at most one | The image a build that names no target builds. |
| `pr-image` | a string, at most one | The image a pull request builds. |

### `workflows`

The shipped workflows this repository turns off, named by file stem.

*at most one, never empty*

#### `<name>`

One workflow, named by the node, and whether it runs.

| Property | Value | Meaning |
| --- | --- | --- |
| `enabled=` | `#true` or `#false`, required | Whether the workflow runs at all. |

<!-- /schema: repo -->

## Image files

Every root `.kdl` but `repo.kdl` is one image. The file name is yours and
reaches nothing.

| | Where it comes from | Where it goes |
| --- | --- | --- |
| the file name | you | nowhere: not the build, not the artifact |
| `id` | `name` lowercased, or declared | published image, build target, cache tag, os-release `DEFAULT_HOSTNAME` |
| `name` | declared | os-release `NAME`, and `PRETTY_NAME` through its default |

```kdl
image {
    name "Workstation"
    url "https://github.com/owner/workstation"
    issues-url "https://github.com/owner/workstation/issues"

    base "quay.io/fedora/fedora-bootc:44" {
        family "fedora"
        provides "rechunking" "initramfs-generation" "mac-policy"
        provides-file "/usr/bin/bootc" "/usr/bin/systemctl"
    }

    flavours {
        dev
    }

    modules {
        module "core/bootloader"
        module "de/kde-desktop" {
            fonts "JetBrainsMono" "FiraCode"
        }

        flavour "dev" {
            module "dev-tools"
        }
    }
}
```

An image builds one target for its ungated module set and one more per
flavour, published as `<id>` and `<id>-<flavour>`. A flavour is a gate rather
than a second image: a module listed inside `flavour "dev"` builds for that
target and no other, and everything else builds for all of them.

A list entry sets the options its module declares, one child node per option,
named as the module named it.

<!-- schema: image -->

### `image`

One image: what it calls itself, what it builds on, and everything it is made of.

| Node | Takes | Meaning |
| --- | --- | --- |
| `id` | a string, at most one | The machine name: published image, build target, cache tag, os-release DEFAULT_HOSTNAME. Derived from `name` when it is not declared. |
| `name` | a string, exactly one | os-release NAME, which the boot menu and the desktop read. |
| `pretty-name` | a string, at most one | os-release PRETTY_NAME, the full name a user is shown. |
| `url` | a string, at most one | The project's home page, in os-release and the image labels. |
| `issues-url` | a string, at most one | Where a user reports a problem with the image. |

#### `base`

The image every layer builds on, and what building on it may assume.

*a string, exactly one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `family` | a string, exactly one | The base's family, matched against every module's `supports`. |
| `provides` | one or more strings | Capabilities the base satisfies that no module could implement portably. |
| `provides-file` | one or more strings | Absolute paths the base guarantees, which a module may require. |
| `signed` | `#true` or `#false`, at most one | Whether the base publishes a cosign signature. |

#### `flavours`

The flavours this image publishes beside its ungated build.

*at most one, never empty*

##### `<name>`

One flavour, named by the node: a gated module set published as `<image>-<flavour>`.

| Property | Value | Meaning |
| --- | --- | --- |
| `default=` | `#true` or `#false` | Whether a build that names no flavour builds this one. |
| `pr-build=` | `#true` or `#false` | Whether a pull request builds this flavour rather than the default. |

#### `modules`

Every module the image is made of: ungated entries, and the flavours that gate the rest.

*exactly one*

##### `module`

One module the image is made of, named by its path under `modules/`.

*a string*

| Property | Value | Meaning |
| --- | --- | --- |
| `variant=` | a string | Which of the module's declared variants this image builds. |

Also holds [`source`](#source).

| Node | Takes | Meaning |
| --- | --- | --- |
| `<name>` |  | An option the module declares, set for this image by the node's name. |

##### `flavour`

The modules one flavour adds, which build only for that flavour.

*a string*

Also holds `module`, as above.

<!-- /schema: image -->

## Out-of-tree modules

A list entry may name a module that lives in another repository. It is
fetched, verified against the hash, and unpacked under `modules/.remote/`, so
its name is one path segment rather than a path.

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

The URL is `https` or `file`, points at a tar archive, and expands `{ref}` and
nothing else. A fetched module ships the same `module.kdl` as any other and is
held to the same schema. Nothing it requires is fetched with it: an
out-of-tree module that needs another one needs that one listed too.

<!-- schema: source -->

### `source`

Where a module that lives outside this repository is fetched from, and what pins it.

*a string*

Also holds [`renovate`](#renovate) and [`manual`](#manual).

| Node | Takes | Meaning |
| --- | --- | --- |
| `ref` | a string, at most one | The tag or commit the archive is fetched at. |
| `sha256` | a string, at most one | What the fetched archive is verified against. |
| `path` | a string, at most one | The module's directory inside the archive. |

<!-- /schema: source -->

## Module manifests

`module.kdl` is the module's whole interface: what it builds on, what it needs
from the rest of the image, and what an image author may set. Everything else
in the directory is convention.

| Path | What the build does with it |
| --- | --- |
| `repo` | sourced first, unless its `REPO_ID` is already configured |
| `module.sh` | sourced as the install logic |
| `selinux/*.te` | compiled and installed, which needs `requires "mac-policy"` |
| `files/` | copied over `/` |
| `finalize.sh` | sourced by the finalize phase, in resolved order |
| `Containerfile.inc` | placed verbatim by `fragment` |
| a file another module collects | staged for it |

```kdl
description "kvmfr DKMS module for GPU passthrough"

supports "fedora"

requires "kernel-devel"
after "vfio"

secret "mok_privkey"
arg "KERNEL"
```

Build order is resolved from `requires` and `after`, never from the order of
the list, which is only a tie-break. A `requires` nothing provides fails the
check and names every module that would satisfy it; an `after` nothing
provides is ignored.

`collects` claims a filename across the whole image and `contributes` says
this module ships one. Each contribution is staged as
`<into>.d/NNNN-<module>.part` and the finalize phase assembles them in that
order, so what the assembled file looks like does not depend on when its
contributors built.

<!-- schema: module -->

Also holds [`option`](#option), [`variant`](#variant) and [`asset`](#asset).

| Node | Takes | Meaning |
| --- | --- | --- |
| `description` | a string, at most one | One line naming the module in the resolved build summary. |
| `supports` | one or more strings | The base families this module builds on, matched against the image's `family`. |
| `provides` | one or more strings | A capability this module satisfies for the modules that require it. |
| `requires` | one or more strings | A capability another module has to provide, which also orders the build. |
| `after` | one or more strings | A module this one builds after without requiring anything of it. |
| `requires-file` | one or more strings | An absolute path some other module has to ship. |
| `overrides` | one or more strings | An absolute path this module replaces deliberately. |
| `secret` | one or more strings | A build secret this module's layer mounts. |
| `arg` | one or more strings | A build argument this module's layer reads. |

### `provides-file`

An absolute path this module guarantees, which another module may require.

*one or more strings*

| Property | Value | Meaning |
| --- | --- | --- |
| `build-only=` | `#true` or `#false` | Whether the path exists only while the build runs. |

### `allow-verify`

One `tect validate-image` diagnostic accepted on one unit rather than image-wide.

*a string*

| Property | Value | Meaning |
| --- | --- | --- |
| `unit=` | a string | The unit the exception applies to. |

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
| `position=` | `before`, `after` | Whether the fragment goes above or below the generated block. |
| `standard-layer=` | `#true` or `#false` | Whether the generated block is emitted at all. |

### `packages`

The packages this module installs, listed per base family.

#### `<name>`

One base family, and the packages to install on it.

*one or more strings*

| Property | Value | Meaning |
| --- | --- | --- |
| `enablerepo=` | a string | A repository enabled for this install and disabled otherwise. |

<!-- /schema: module -->

## Options

An option is one value an image may set on the module that declared it. Every
declared option reaches that module's layer as `OPT_<NAME>`, uppercased with
dashes as underscores, always, defaults included, so `module.sh` reads a
variable rather than testing whether one is set.

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

## Asset pins

An asset is an upstream payload the module fetches during its own layer,
pinned by version and by hash. Its fields reach that layer as
`ASSET_<NAME>_VERSION`, `_URL` and `_SHA256`, with `{version}` already
expanded, so nothing in shell derives a URL.

```kdl
asset "starship" {
    renovate datasource="github-releases" depName="starship/starship"
    version "1.26.0"
    url "https://github.com/starship/starship/releases/download/v{version}/starship-x86_64-unknown-linux-musl.tar.gz"
    sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3" from="sidecar"
}
```

<!-- schema: asset -->

### `asset`

A pinned upstream payload the module fetches, reaching the build as ASSET_*.

*a string, one per name*

Also holds [`renovate`](#renovate) and [`manual`](#manual).

| Node | Takes | Meaning |
| --- | --- | --- |
| `version` | a string, at most one | The pinned version, which the URL expands and Renovate rewrites. |
| `url` | a string, at most one | Where the payload is fetched from. |

#### `sha256`

What the fetched payload is verified against.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `from=` | `asset`, `sidecar`, `manual` | Where the hash is refreshed from. |

<!-- /schema: asset -->

## Keeping a pin current

An asset and an out-of-tree module are pinned the same way, and both have to
say how the pin is kept current: `renovate` for one a bot bumps, `manual` for
one nothing tracks, and why. Exactly one of the two, because a pin that says
neither goes stale in silence.

Renovate matches `renovate` together with the line directly below it, so the
`version` or `ref` it rewrites has to sit there with nothing in between.

<!-- schema: renovate -->

### `renovate`

The custom manager Renovate matches to keep the pin current.

*at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `datasource=` | `github-releases`, `github-tags`, `git-refs`, required | Which Renovate datasource the pin is tracked through. |
| `depName=` | a string, required | What that datasource calls the thing being tracked. |
| `extractVersion=` | a string | The pattern Renovate pulls the version out of the tag with. |

<!-- /schema: renovate -->

<!-- schema: manual -->

### `manual`

Why nothing tracks this pin.

*a string, at most one*

<!-- /schema: manual -->
