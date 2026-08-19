# The schema

Three kinds of file, all KDL, all read by `tect`.

| File | Declares |
| --- | --- |
| `repo.kdl` | the repository: the schema it is written against, the tool release it pins, which image a bare build builds, which shipped workflows run |
| `image.kdl` or `<name>.image.kdl` at the root | one image file, holding what each image calls itself, what it builds on, and the modules in it |
| `modules/<path>/module.kdl` | one module: what it needs, what it offers, and what an image author may set |

`tect check` reads all three and reports every problem at the line that caused
it. The reference under each heading below is generated from the tables the
parser walks, so it cannot drift from what the tool accepts.

## The repository file

`repo.kdl` holds what is true of the repository rather than of any image in
it.

```kdl
schema-version 1

// renovate: datasource=github-releases depName=tectonic-os/tectonic
tect-version "0.0.0"

default-image "workstation"
pr-image "workstation"

workflows {
    smoke-test enabled=#false
}
```

`schema-version` picks the reader, so a repository written against an earlier
release keeps building: a tool that does not know the version says so plainly
instead of reporting every node it cannot place. A repository behind this
release is moved forward by `tect update-repo`; one ahead of it is read by the
release it pins.

`tect-version` is that release. `scripts/tect.sh` fetches it, so the build
runs the tool the repository was written for whatever is installed on the
machine, and every command refuses to run in a repository pinned to a release
it is not. A repository that pins nothing is held to nothing.

A workflow is named by its file stem under `.github/workflows/`, and one
nobody names runs. The block is how a repository turns something off.

`sources` is the registry `tect import module <name>` resolves against. Each
collection is named by the owner its modules land under, so
`import flatpak` from the collection below writes
`modules/tectonic-os/flatpak`, and the image lists it as
`module "tectonic-os/flatpak"`.

```kdl
sources {
    tectonic-os {
        pin {
            renovate datasource="github-tags" depName="tectonic-os/modules"
            version "v1.0.0"
            url "https://github.com/tectonic-os/modules/archive/refs/tags/{version}.tar.gz"
            sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
        }
    }
    scratch "../modules"
}
```

A collection carrying a `pin` is an archive, fetched and verified exactly like
an out-of-tree module. One carrying a location instead is a directory on this
machine, relative to the repository root, which is read where it is: nothing is
downloaded, so there is nothing to pin or hash. That is what makes iterating on
a collection possible without re-tarring it on every edit.

A name here is checked when it is imported from, not when the repository is
checked: a directory that exists on one machine and not another is not a
repository problem.

### Collections that are not pinned

A collection may instead follow a branch, and `unpinned` is what says so. That
is what `create repo` scaffolds, and it buys convenience with verification:

```kdl
sources {
    tectonic-os {
        pin {
            unpinned "the collection this tool is published alongside, followed at its branch head"
            version "main"
            url "https://github.com/tectonic-os/modules/archive/refs/heads/{version}.tar.gz"
        }
    }
}
```

Every `import module` from an unpinned collection downloads the branch again
and takes whatever it holds at that moment. There is no `sha256`, so nothing
checks what arrived: a mistaken commit, or a compromised one, lands with
nothing to catch it. What limits the damage is that the import is a copy into
your tree, which you read and commit like any other change, rather than
something the build fetches and runs. Tagging the collection instead would
verify every fetch, at the cost of versioning every module in it together.

`unpinned` is the third answer beside `renovate` and `manual` and excludes
both: it says the ref moves on its own. It cannot be combined with a `sha256`,
which a moving ref would break the moment it moved, and no other holder of a
pin may carry it at all — the build fetches an out-of-tree module and runs it
as root without anyone reading it first, so its hash is not optional. A pin
with neither a `sha256` nor `unpinned` is still an error, so a hash that is
dropped or mistyped is reported rather than quietly becoming trust in whatever
answers the URL.

`tect check` names every unpinned collection above its counts. It is not an
error; it is the one thing that stops the repository being reproducible, and
it is worth being reminded of.

`audit` is the posture, and only the posture. Every provenance fact is recorded
whether or not it is declared: what a module hashed to, where it was imported
from, what the base tag resolved to, what commit a cloned asset was taken at.
`enforce #true` decides which of those being missing or not matching stops the
run.

```kdl
audit {
    enforce #true
}
```

Under enforcement these are errors rather than read-outs: importing from a
collection that follows a moving ref with no `sha256`, a module whose content no
longer matches the record it was imported with, a base tag that will not resolve
to a manifest digest, and a repository at no commit. Without it the same
repository checks and builds clean and the same facts are written down, which is
what stops a built artifact ever implying an audit it did not get.

One diagnostic is on either way: a module whose `module.sh` or `finalize.sh`
reaches the network with no `asset` declaring what it pulls. An undeclared fetch
is the one build input no record can describe after the fact, because nothing
says what it should have been.

`seed` nominates the image a new repository may start from, and `generate`
writes it to `generated/seed.kdl`.

```kdl
seed "workstation" collection="tectonic-os"
```

The seed carries the base, the module list and the collections those modules
come from, and nothing about this repository: no name, no URL, no owner. Each
module is named by the collection it is fetched through, which is why
`collection` is required and has to be one of the collections in `sources`: a
repository is seedable only if it publishes its own `modules/` as a collection
too. An image listing a module nothing can import that way is reported, since
the seed of it would leave a new repository unbuildable.

<!-- schema: repo -->

| Node | Takes | Meaning |
| --- | --- | --- |
| `schema-version` | a number, at most one | The schema release this repository is written against, which picks the reader. |
| `tect-version` | a string, at most one | The tect release this repository is built with, which every command holds itself to. |
| `default-image` | a string, at most one | The image a command given no image answers about, and a build with no target builds. |
| `pr-image` | a string, at most one | The image a pull request builds. |

### `seed`

The image this repository publishes a declaration of, for a new repository to start from.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `collection=` | a string, required | The collection this repository publishes its own modules as, which is what names them in the seed. |

### `workflows`

The shipped workflows this repository turns off, named by file stem.

*at most one, never empty*

#### `<name>`

One workflow, named by the node, and whether it runs.

| Property | Value | Meaning |
| --- | --- | --- |
| `enabled=` | `#true` or `#false`, required | Whether the workflow runs at all. |

### `sources`

The module collections `tect import module` resolves a name against.

*at most one, never empty*

#### `<name>`

One module collection, named by the owner its modules land under in modules/.

*a string*

Also holds [`pin`](#pin).

### `manifest`

Whether a build stamps the generated manifest onto the image as an OCI label.

*at most one, never empty*

| Node | Takes | Meaning |
| --- | --- | --- |
| `label` | `#true` or `#false`, at most one | Whether the build stamps `org.tectonic.manifest` with the path to the baked manifest file. |

### `audit`

How strictly the provenance facts are held. Every one of them is recorded either way; this decides only which of them is fatal.

*at most one, never empty*

| Node | Takes | Meaning |
| --- | --- | --- |
| `enforce` | `#true` or `#false`, at most one | Whether a provenance fact that is missing or does not match stops the run rather than being reported. |

<!-- /schema: repo -->

## Image files

A root `.kdl` is an image only when it is named `image.kdl` or ends in
`.image.kdl`. Any other root `.kdl` is reported with a diagnostic rather than
read. One image file may hold more than one `image` node.

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

`base { provides }` is what the upstream image already ships. A listed module
whose every `provides` and `provides-file` the base already carries is
suppressed: it is not ordered, not built, and nothing it ships reaches the
image, since its layer would provision what is already there a second time.
The generated graph lists what was suppressed and `plan.json` carries it
beside the modules that did build. A module the base covers only in part still
builds, and declaring what the base already provides is an error there.

Options and variants on a suppressed module still resolve, so a value set on
one is still checked and still reaches the plan; it just reaches no layer.

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
| `description` | a string, at most one | A one-line summary of the image, in its OCI labels and not in os-release. |
| `keywords` | one or more strings | Keywords for the image's OCI labels, comma-joined into one label. |
| `logo-url` | a string, at most one | A URL to the image's logo, in its OCI labels. |
| `conforms` | a string, at most one | The benchmark profile a scan measures this image against, reported rather than enforced. |

#### `base`

The image every layer builds on, and what building on it may assume.

*a string, exactly one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `family` | a string, exactly one | The base's family, matched against every module's `supports`. |
| `provides` | one or more strings | Capabilities the upstream image already ships; a module providing only these is suppressed. |
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

Also holds [`pin`](#pin).

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
    pin {
        renovate datasource="github-tags" depName="owner/bootc-modules"
        version "steam-tweaks/v1.2.0"
        url "https://github.com/owner/bootc-modules/archive/refs/tags/{version}.tar.gz"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
        path "modules/steam-tweaks"
    }
}
```

The URL is `https` or `file`, points at a tar archive, and expands `{version}`
and nothing else. A fetched module ships the same `module.kdl` as any other and is
held to the same schema. Nothing it requires is fetched with it: an
out-of-tree module that needs another one needs that one listed too.

## The base catalog

`tect create image` offers the bases it knows: what family each belongs to and
what it already ships, so an image scaffolded on one lists no module the base
already carries. The tool compiles one `bases.kdl` into the binary as a
fallback, and the release ships that same file for runtime replacement: a
runtime `bases.kdl` beside the binary, when present, replaces the compiled-in
catalog entirely rather than merging with it; a present but unreadable or
malformed runtime file is diagnosed by its exact path instead of silently
falling back to the compiled-in one. A collection then extends the selected
catalog with a `bases.kdl` at its root, beside the modules.

```kdl
base "ghcr.io/ublue-os/bazzite:stable" {
    about "KDE, gaming and hardware support over kinoite-main"
    family "fedora"
    provides "rechunking" "flatpak"
    provides-file "/usr/bin/flatpak"
    signed #true
}
```

A collection entry wins over the selected catalog's entry of the same
reference, which is how a stale one is corrected without a tool release, and
`check` names the collection wherever the two differ. A base two collections
describe is an error, the way a collection declared twice is. Nothing is
fetched to read one: a collection that is not on this machine already extends
nothing, and the selected catalog is what the picker offers.

<!-- schema: bases -->

### `base`

One base a collection describes, named by the reference an image builds on.

*a string, one per name*

| Node | Takes | Meaning |
| --- | --- | --- |
| `about` | a string, exactly one | The line a base picker shows beside the reference. |
| `family` | a string, exactly one | The family an image built on this base declares, matched against every module's `supports`. |
| `provides` | one or more strings | Capabilities this base already ships, written into every image scaffolded on it. |
| `provides-file` | one or more strings | Absolute paths this base guarantees, written into every image scaffolded on it. |
| `signed` | `#true` or `#false`, at most one | Whether this base publishes a cosign signature, which a scaffolded image records. |

<!-- /schema: bases -->

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
provides is ignored. A capability name is lowercase letters, digits and
dashes, starting with a letter, since the generated graph writes it into a
mermaid label and a markdown table cell without quoting it.

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

### `key`

A key `tect create key` generates for this module, and where each half of it goes.

*a string, one per name*

| Node | Takes | Meaning |
| --- | --- | --- |
| `private` | a string, exactly one | What the private half is called at the repository root. |

#### `generator`

Which of the generators the tool implements writes this key.

*`cosign`, `openssl`, exactly one*

| Property | Value | Meaning |
| --- | --- | --- |
| `profile=` | `module-signing` | What the generator is set up for, where it can do more than one thing. |
| `bits=` | 2048 to 16384 | The RSA key size, 4096 where none is named. |

#### `public`

Where the public half is shipped, which is a contract path this module provides.

*a string, exactly one*

| Property | Value | Meaning |
| --- | --- | --- |
| `format=` | `pem`, `der` | What the public half is written as, PEM where none is named. |

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
| `enablerepo=` | a string | A repository enabled for this install and disabled otherwise. Fedora only. |

### `satisfies`

The benchmarks and rules this module claims to harden, as an audit declaration the tool records rather than certifies.

*at most one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `<name>` | one or more strings, one per name | One benchmark, and the rule IDs it covers. |

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

## Asset pins

An asset is an upstream payload the module fetches during its own layer,
pinned by version and by hash. Its fields reach that layer as
`ASSET_<NAME>_VERSION`, `_URL` and `_SHA256`, with `{version}` already
expanded, so nothing in shell derives a URL.

```kdl
asset "starship" {
    pin {
        renovate datasource="github-releases" depName="starship/starship"
        version "1.26.0"
        url "https://github.com/starship/starship/releases/download/v{version}/starship-x86_64-unknown-linux-musl.tar.gz"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3" from="sidecar"
    }
}
```

<!-- schema: asset -->

### `asset`

A pinned upstream payload the module fetches, reaching the build as ASSET_*.

*a string, one per name, never empty*

Also holds [`pin`](#pin).

<!-- /schema: asset -->

## Where something comes from

Four slots answer it, and every pin in the tree fills the same four: the
locator says where it comes from, the selector which version of it, the
verifier what proves you got that one, and the tracker who keeps the selector
current. One `pin` block carries them, and `asset`, an out-of-tree module and a
collection each hold it. A base carries its locator and selector joined in the
image reference, and `signed` as its verifier.

Every pin has to say how it is kept current: `renovate` for one a bot bumps,
`manual` for one nothing tracks, and why. Exactly one of them, because a pin
that says neither goes stale in silence. A collection has a third answer,
`unpinned`, which is the only one that leaves the content unverified.

Renovate matches `renovate` together with the line directly below it, so the
`version` it rewrites has to sit there with nothing in between.

<!-- schema: pin -->

### `pin`

Where this comes from, which version of it, what proves you got that one, and what keeps the selector current.

*at most one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `manual` | a string, at most one | Why nothing tracks this pin. |
| `unpinned` | a string, at most one | Why this follows a moving ref with no `sha256`, so every fetch takes whatever the ref holds then and nothing checks what arrived. |
| `version` | a string, at most one | The selector: the version, tag or commit this is taken at, which the URL expands and Renovate rewrites. |
| `url` | a string, at most one | The locator: where the content comes from. |
| `path` | a string, at most one | The directory inside the archive the content sits in. |

#### `renovate`

The custom manager Renovate matches to keep the selector current.

*at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `datasource=` | `github-releases`, `github-tags`, `git-refs`, required | Which Renovate datasource the pin is tracked through. |
| `depName=` | a string, required | What that datasource calls the thing being tracked. |
| `extractVersion=` | a string | The pattern Renovate pulls the version out of the tag with. |

#### `sha256`

The verifier: what the fetched content is held to.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `from=` | `asset`, `sidecar`, `manual` | Where the hash is refreshed from. |

<!-- /schema: pin -->

## The import record

`tect import module` copies a module in and writes a `provenance.kdl` beside
its `module.kdl`, recording which collection it came from, what pinned that
collection, and what the directory hashed to. It is a sibling file and never
part of `module.kdl`: that is the author's file, and rewriting it on import
would fork it from upstream and break the very comparison the hash exists to
make. The record excludes itself from its own hash, so what `content` names is
the directory except this one file.

```kdl
imported "tectonic-os" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    pin {
        unpinned "the collection this tool is published alongside, followed at its branch head"
        version "main"
        url "https://github.com/tectonic-os/modules/archive/refs/heads/{version}.tar.gz"
    }
}
```

`plan.json` carries the same hash per module, so `verify` fails on a module
edited without regenerating. `tect check` names a module whose content no
longer matches its record. That is not an error: forking an imported module is
legitimate, and what the record buys is that the fork is visible rather than
silent.

<!-- schema: imported -->

### `imported`

Where this module was copied from, and what its content hashed to then. Written by `tect import module`; the module's author does not maintain it.

*a string, exactly one*

Also holds [`pin`](#pin).

| Node | Takes | Meaning |
| --- | --- | --- |
| `content` | a string, exactly one | What the module directory hashed to when it was imported, every file in it except this one. |

<!-- /schema: imported -->
