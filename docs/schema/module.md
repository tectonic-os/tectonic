<a id="module.kdl"></a>

# `module.kdl`

Location: `modules/<module-name>/module.kdl`, in the directory of the module.

A module is one reusable part of an image, such as a desktop, a kernel or a set of tools. `module.kdl` declares what the module needs and provides, what it installs and which families it supports, and the files beside it do the work.

```kdl
description "Traditional CLI utilities"
supports "fedora" "debian"
packages "htop" "tmux"
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`description`](#description) (optional) | *string* | One line that names the module in the resolved build summary. |
| [`supports`](#supports) (optional,&nbsp;repeatable) | *list of strings* | The base families that this module builds on, which the image's `family` must match. Omit it to support every family. |
| [`provides`](#provides) (optional,&nbsp;repeatable) | *list of strings* | A capability that this module provides for the modules that require it. |
| [`requires`](#requires) (optional,&nbsp;repeatable) | *list of strings* | A capability that another module must provide, which also orders the build. |
| [`after`](#after) (optional,&nbsp;repeatable) | *list of strings* | A capability that this module builds after, which the module does not require. |
| [`overrides`](#overrides) (optional,&nbsp;repeatable) | *list of strings* | An absolute path that this module replaces on purpose. |
| [`mode`](#mode) (optional,&nbsp;unique) | *path*, then *octal mode* | The octal file mode that one path in the overlay of this module takes. |
| [`key`](#key) (optional,&nbsp;unique) | *string*, then {&nbsp;[fields](#key-fields)&nbsp;} | A key that `tect create key` generates for this module. |
| [`secret`](#secret) (optional,&nbsp;repeatable) | *list of strings* | A build secret that the layer of this module mounts. |
| [`arg`](#arg) (optional,&nbsp;repeatable) | *list of strings* | A build argument that the layer of this module reads. |
| [`helpers`](#helpers) (optional,&nbsp;repeatable) | *list of strings* | Files from this module that every module layer mounts by basename into `/ctx/lib`. |
| [`allow-verify`](#allow-verify) (optional,&nbsp;repeatable) | *string* | One `tect validate-image` diagnostic that the check accepts on one unit, while the check still covers every other diagnostic and unit of the image. |
| [`refuses`](#refuses) (optional,&nbsp;unique) | *string* | One benchmark rule that this module leaves unsatisfied on purpose. |
| [`collects`](#collects) (optional,&nbsp;repeatable) | *string* | A filename that this module gathers from every module that ships one. A collector claims one filename across the image. |
| [`contributes`](#contributes) (optional,&nbsp;repeatable) | *string* | A file that this module ships for another module to collect. |
| [`fragment`](#fragment) (optional) |  | Where the module's `Containerfile.inc` goes in relation to the generated layer. |
| [`option`](#option) (optional,&nbsp;unique) | *string*, then {&nbsp;[fields](#option-fields)&nbsp;} | One value that an image can set on this module. |
| [`variant`](#variant) (optional,&nbsp;unique) | *string*, then optionally {&nbsp;[fields](#variant-fields)&nbsp;} | A named set of option values that an image selects with `variant=`. |
| [`asset`](#asset) (optional,&nbsp;unique) | *string*, then {&nbsp;[fields](#asset-fields)&nbsp;} | A pinned upstream payload that the module fetches during its own layer. |
| [`network`](#network) (optional) | `scripts` | The declaration that the script step of this module reaches the network. |
| [`packages`](#packages) (optional,&nbsp;repeatable) | *list of strings* | The packages that this module installs. |
| [`package-groups`](#package-groups) (optional,&nbsp;repeatable) | *list of strings* | The package groups that this module installs. Package groups work with dnf only, so an ungated `package-groups` needs a module that supports only `fedora` or `rhel`. |
| [`copr`](#copr) (optional,&nbsp;unique) | *string* | The `owner/project` name of a COPR repository that this module enables for its own installs. `copr` works on Fedora only. |
| [`satisfies`](#satisfies) (optional) | optionally {&nbsp;[fields](#satisfies-fields)&nbsp;} | An audit declaration of the benchmarks and rules that this module claims to harden. `tect` records the claim and certifies nothing, and the scan checks it after the build. |
| [`family`](#family) (optional,&nbsp;repeatable) | *list of strings*, then {&nbsp;[fields](#family-fields)&nbsp;} | The declarations that the build takes only on the named base families. |

> [!NOTE]
> `requires` and `after` decide the build order. The order of the image list only breaks ties.
>
> If nothing provides a `requires` or an `after`, then `tect check` fails and names every module that would satisfy it.

<a id="description"></a>

## `description` (optional)

One line that names the module in the resolved build summary.

```kdl
description "Traditional CLI utilities"
```

Accepts: *string*

<a id="supports"></a>

## `supports` (optional, repeatable)

Lists the base families that the module works on. `tect check` refuses the module on an image whose base family is not in the list, so a portability gap shows before the build. A module with no `supports` declaration is base-agnostic and supports every family.

```kdl
supports "fedora" "debian"
```

Accepts: *list of strings*

<a id="provides"></a>

## `provides` (optional, repeatable)

Names a capability that this module adds to the image, such as `flatpak`. Another module `requires` it by name, and the build checks the finished image for the file that witnesses it.

```kdl
provides "flatpak"
```

Accepts: *list of strings*

| Property | Accepts | Description |
| --- | --- | --- |
| `file=` (optional) | *string* | The absolute path that witnesses the one capability that the node names. It is needed only if the witness is neither `/usr/bin/<name>` nor `/usr/sbin/<name>`. |
| `build-only=` (optional) | *boolean* | Whether the `file` exists only while the build runs. |

> [!NOTE]
> The build checks the finished image for the witness file.
>
> A capability name is lower-case letters, digits and dashes, and starts with a letter. The generated graph writes the name unquoted into a mermaid label and a markdown table cell.

<a id="requires"></a>

## `requires` (optional, repeatable)

Names a capability that the image must have for this module to work. `tect` builds this module after the module that provides it, and `tect check` fails if no module in the image provides it.

```kdl
requires "kernel-devel"
```

Accepts: *list of strings*

<a id="after"></a>

## `after` (optional, repeatable)

A capability that this module builds after, which the module does not require.

```kdl
after "vfio"
```

Accepts: *list of strings*

<a id="overrides"></a>

## `overrides` (optional, repeatable)

Declares that this module replaces a file that an earlier module also ships. Without it, `tect check` reports the two overlays as a collision. `tect check` also reports an override that no earlier module collides with, so an override cannot outlive its reason.

```kdl
overrides "/etc/containers/policy.json"
```

Accepts: *list of strings*

<a id="mode"></a>

## `mode` (optional, unique)

Sets the file mode of one file that the `files/` overlay of the module installs, such as `0600` for a file that only root reads.

```kdl
mode "/etc/audit/rules.d/audit.rules" "0600"
```

Accepts: *path*, then *octal mode*

<a id="key"></a>

## `key` (optional, unique)

A module that needs a key declares it here, such as a Secure Boot signing key. `tect create key` generates the key from this declaration. The build installs the public half into the image at the path that `public` names. The private half stays in `keys/private/`, outside the image and outside git.

```kdl
key "secureboot" {
    generator "openssl" profile="module-signing"
    public "/usr/share/secureboot/sb_cert.der" format="der"
    private "MOK.priv"
}
```

Accepts: *string*, then {&nbsp;[fields](#key-fields)&nbsp;}

<a id="key-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`generator`](#key-generator) (required) | `cosign` or `openssl` or `ssh-keygen` | The generator that writes this key. |
| [`public`](#key-public) (required) | *string* | Where the public half ships, which witnesses the `<kind>-key` capability of this module. |
| [`private`](#key-private) (required) | *string* | The filename that the private half takes under `keys/private/`. |

<a id="key-generator"></a>

### `generator` (required)

`tect create key` offers every key that a module declares. `generator` names the tool that writes this key, and its properties set the key up for what the module uses it for.

```kdl
key "secureboot" {
    generator "openssl" profile="module-signing"
}
```

Accepts: `cosign` or `openssl` or `ssh-keygen`

| Property | Accepts | Description |
| --- | --- | --- |
| `profile=` (optional) | `module-signing` or `pcr-signing` or `tls-ca` | The purpose that the generator serves, if the generator has more than one purpose. |
| `bits=` (optional) | *number* from 2048 to 16384 | The RSA key size, which is 4096 by default. |

<a id="key-public"></a>

### `public` (required)

Where the build installs the public half in the image. The file also witnesses the `<kind>-key` capability, so another module can `requires` the key.

```kdl
key "secureboot" {
    public "/usr/share/secureboot/sb_cert.der" format="der"
}
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `format=` (optional) | `pem` or `der` | The format of the public half, which is PEM by default. |

<a id="key-private"></a>

### `private` (required)

The file name of the private half under `keys/private/`, which git ignores. The build never copies the private half into the image.

```kdl
key "secureboot" {
    private "MOK.priv"
}
```

Accepts: *string*

<a id="secret"></a>

## `secret` (optional, repeatable)

Names a build secret that the layer of this module reads, such as a signing key. The layer mounts it at `/run/secrets/<name>`, so the secret is there while the module builds and never lands in the image.

```kdl
secret "mok_privkey"
```

Accepts: *list of strings*

<a id="arg"></a>

## `arg` (optional, repeatable)

Names a build argument that the layer of this module reads as an environment variable, such as the kernel version to install.

```kdl
arg "KERNEL"
```

Accepts: *list of strings*

<a id="helpers"></a>

## `helpers` (optional, repeatable)

Shares shell helpers from this module with every other module. The build mounts each listed file by its basename into `/ctx/lib` in every module layer, so another module can source it.

```kdl
helpers "lib/family.sh"
```

Accepts: *list of strings*

<a id="allow-verify"></a>

## `allow-verify` (optional, repeatable)

`tect validate-image` checks the systemd units of the finished image. If one unit of this module raises an expected diagnostic, then this accepts that one diagnostic on that one unit. The check still covers every other diagnostic and unit.

```kdl
allow-verify "man-page-missing" unit="plasmalogin.service"
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `unit=` (optional) | *string* | The unit the exception applies to. |

<a id="refuses"></a>

## `refuses` (optional, unique)

A hardening rule can break what a module does. This records that the module leaves one rule unsatisfied on purpose, and `because=` says why. Remediation then does not set the rule, and the choice is visible to every image.

```kdl
refuses "grub2_nousb_argument" because="it removes the keyboard"
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `because=` (optional) | *string* | Why the module leaves the rule unsatisfied. |

> [!NOTE]
> Remediation does not set a refused rule. If an image declares `allow-remediation` for the rule, then remediation can set it.

<a id="collects"></a>

## `collects` (optional, repeatable)

Builds one file from parts that many modules ship, such as a list of Flatpak apps. This module claims the file name, every module that ships a file of that name contributes a part, and the finalize phase assembles the parts at `into=`.

```kdl
collects "flatpaks.list" into="/usr/share/flatpak-defaults/apps.list" priority=500
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `into=` (optional) | *string* | The absolute path that receives the assembled file. |
| `priority=` (optional) | *number* from 0 to 9999 | The position of a contribution that declares no priority. |

> [!NOTE]
> The build stages each contribution as `<into>.d/NNNN-<module>.part`. The finalize phase assembles the parts in that order, so the build order of the contributors does not change the assembled file.
>
> A module ships at most one copy of a collected file. The most specific copy wins. The order is `debian/`, then `deb/`, then the module root.

<a id="contributes"></a>

## `contributes` (optional, repeatable)

A file that this module ships for another module to collect.

```kdl
contributes "flatpaks.list"
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `priority=` (optional) | *number* from 0 to 9999 | The position that this file takes in the assembled file. |

<a id="fragment"></a>

## `fragment` (optional)

A module whose needs the fields cannot express ships a `Containerfile.inc`, which the build adds verbatim. This places those lines relative to the generated layer.

```kdl
fragment position="after"
```

| Property | Accepts | Description |
| --- | --- | --- |
| `position=` (optional) | `before` or `after` or `tail` | Where the fragment goes. `before`, the default, puts it above the generated block. `after` puts it below the block. `tail` puts it below the finalize layer, where the lineage stage has ended and can be named. |
| `standard-layer=` (optional) | *boolean* | Whether the build emits the generated block. |

> [!NOTE]
> The build places the fragment verbatim and replaces `@MODULE@` with `/modules/<dir>`, the module directory in the build context. The mounts of the module layer use the same path, so a `COPY --from=ctx` needs it to reach a file that the module ships.
>
> Every `RUN` bind-mounts over `/etc/hostname`, `/etc/hosts` and `/etc/resolv.conf`. A fragment writes those three files with a `COPY --from=ctx`.

<a id="option"></a>

## `option` (optional, unique)

Lets an image change how this module builds, such as which fonts it installs. The module declares the option with a type and a default, and each image can set its own value.

```kdl
option "fonts" type="list" {
    default "JetBrainsMono" "FiraCode"
}
```

Accepts: *string*, then {&nbsp;[fields](#option-fields)&nbsp;}

| Property | Accepts | Description |
| --- | --- | --- |
| `type=` (required) | `string` or `bool` or `list` | The type of the value. A `string` arrives verbatim, a `bool` arrives as `1` or `0`, and a `list` arrives as a bash array of its strings. |

<a id="option-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`description`](#option-description) (optional) | *string* | What the option does. |
| [`default`](#option-default) (required) | *list of strings* | The value that the module builds with if no image sets one. |

> [!NOTE]
> Every declared option reaches the layer of its module as `OPT_<NAME>`. `<NAME>` is the option name in upper case, and each dash becomes an underscore.
>
> A default reaches the layer too, so `module.sh` reads a variable and does not test whether one is set.

<a id="option-description"></a>

### `description` (optional)

What the option does.

```kdl
option "fonts" type="list" {
    description "Nerd Font families to install"
}
```

Accepts: *string*

<a id="option-default"></a>

### `default` (required)

The value that the module builds with if no image sets one.

```kdl
option "fonts" type="list" {
    default "JetBrainsMono" "FiraCode"
}
```

Accepts: *list of strings*

<a id="variant"></a>

## `variant` (optional, unique)

Bundles option values under one name, so an image takes a whole configuration of the module with `variant=` in place of setting each option.

```kdl
variant "wine-only"
```

Accepts: *string*, then optionally {&nbsp;[fields](#variant-fields)&nbsp;}

<a id="variant-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`description`](#variant-description) (optional) | *string* | What the variant is for. |
| [`set`](#variant-set) (optional,&nbsp;repeatable) | *string* | One option that this variant sets, where the option value follows the option name. |

> [!NOTE]
> An image can still set an option on an entry that selects a variant. The option that the image sets wins.

<a id="variant-description"></a>

### `description` (optional)

What the variant is for.

```kdl
variant "wine-only" {
    description "Skip the metadata and .NET payloads"
}
```

Accepts: *string*

<a id="variant-set"></a>

### `set` (optional, repeatable)

One option that this variant sets, where the option value follows the option name.

```kdl
variant "wine-only" {
    set "dotnet" #false
}
```

Accepts: *string*

<a id="asset"></a>

## `asset` (optional, unique)

Downloads a pinned file from upstream, such as a release binary, while this module builds. The pin fixes the version and the hash, so every build gets the same bytes and Renovate can keep the version current.

```kdl
asset "starship" {
    pin {
        renovate datasource="github-tags" depName="owner/repo"
        version "v1.0.0"
        url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    }
}
```

Accepts: *string*, then {&nbsp;[fields](#asset-fields)&nbsp;}

The layer of the module gets three variables, where `<NAME>` is the asset name in upper case with each dash turned into an underscore:

- `ASSET_<NAME>_VERSION`;
- `ASSET_<NAME>_URL`, with `{version}` already expanded, so no shell code builds a URL;
- `ASSET_<NAME>_SHA256`.

<a id="asset-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`pin`](#asset-pin) (optional) | {&nbsp;[fields](#asset-pin-fields)&nbsp;} | Where content comes from, which version it is, what verifies it, and what keeps the version current. |

<a id="asset-pin"></a>

### `pin` (optional)

A pin fixes where a download comes from and which version it is, and says how the version stays current. Unless the pin is `unpinned`, `tect` verifies each download against the pinned hash, so a changed upstream file is caught.

```kdl
asset "starship" {
    pin {
        renovate datasource="github-tags" depName="owner/repo"
        version "v1.0.0"
        url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    }
}
```

Accepts: {&nbsp;[fields](#asset-pin-fields)&nbsp;}

A pin holds exactly one of these, so every pin says how it stays current:

- `renovate`, if Renovate keeps the version current;
- `manual`, if nothing tracks the pin;
- `unpinned`, if the pin follows a moving ref with no `sha256`.

<a id="asset-pin-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`renovate`](#asset-pin-renovate) (optional) |  | The Renovate custom manager that keeps `version` current. |
| [`manual`](#asset-pin-manual) (optional) | *string* | Why nothing tracks this pin. |
| [`unpinned`](#asset-pin-unpinned) (optional) | *string* | Why this pin follows a moving ref with no `sha256`. |
| [`version`](#asset-pin-version) (required) | *string* | The version, tag or commit that `url` expands and Renovate rewrites. |
| [`url`](#asset-pin-url) (required) | *string* | Where the content comes from. |
| [`sha256`](#asset-pin-sha256) (optional) | *string* | The hash the fetched content must match. |
| [`path`](#asset-pin-path) (optional) | *string* | The directory inside the archive that holds the content. |

> [!NOTE]
> An `asset`, an out-of-tree module and a collection each hold a `pin`.
>
> A base holds no `pin`. Its image reference holds the location and the version, and `signed` records whether the base publishes a cosign signature.
>
> Renovate matches `renovate` together with the line directly below it. Put `version` on that line.

<a id="asset-pin-renovate"></a>

#### `renovate` (optional)

The Renovate custom manager that keeps `version` current.

```kdl
asset "starship" {
    pin {
        renovate datasource="github-tags" depName="owner/repo"
    }
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `datasource=` (required) | `github-releases` or `github-tags` or `git-refs` | The Renovate datasource that tracks the pin. |
| `depName=` (required) | *string* | The name the datasource knows the dependency by. |
| `extractVersion=` (optional) | *string* | The pattern that extracts the version from a tag. |

<a id="asset-pin-manual"></a>

#### `manual` (optional)

Why nothing tracks this pin.

```kdl
asset "starship" {
    pin {
        manual "upstream publishes no releases"
    }
}
```

Accepts: *string*

<a id="asset-pin-unpinned"></a>

#### `unpinned` (optional)

Why this pin follows a moving ref with no `sha256`.

```kdl
asset "starship" {
    pin {
        unpinned "followed at its branch head"
    }
}
```

Accepts: *string*

> [!NOTE]
> Every fetch takes what the ref holds at that moment, and nothing checks what arrived.
>
> Only a collection's pin takes `unpinned`. The build runs an out-of-tree module as root, so the pin of an out-of-tree module needs a `sha256`.
>
> `unpinned` does not combine with `sha256`, because a moving ref breaks the hash.

<a id="asset-pin-version"></a>

#### `version` (required)

The version, tag or commit that `url` expands and Renovate rewrites.

```kdl
asset "starship" {
    pin {
        version "v1.0.0"
    }
}
```

Accepts: *string*

<a id="asset-pin-url"></a>

#### `url` (required)

Where the content comes from.

```kdl
asset "starship" {
    pin {
        url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
    }
}
```

Accepts: *string*

<a id="asset-pin-sha256"></a>

#### `sha256` (optional)

The hash the fetched content must match.

```kdl
asset "starship" {
    pin {
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    }
}
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `from=` (optional) | `asset` or `sidecar` or `manual` | Where the hash is refreshed from. `asset`, the default, hashes the payload itself. `sidecar` reads the `<url>.sha256` file that upstream publishes. `manual` means nothing recomputes the hash. |

> [!NOTE]
> If a pin has no `sha256` and no `unpinned`, then `tect check` reports an error.

<a id="asset-pin-path"></a>

#### `path` (optional)

The directory inside the archive that holds the content.

```kdl
asset "starship" {
    pin {
        path "modules/example"
    }
}
```

Accepts: *string*

<a id="network"></a>

## `network` (optional)

Declares that the scripts of this module need the network. A repository with a `strict` script rule opens the network only for the modules that declare it.

```kdl
network "scripts"
```

Accepts: `scripts`

> [!NOTE]
> Only a `strict` script rule in `repo.kdl` reads this node.
>
> The package step needs no declaration, because `packages`, `copr` or a repo file already declares it.

<a id="packages"></a>

## `packages` (optional, repeatable)

Lists the distribution packages that this module installs with the package manager of the base family.

```kdl
packages "htop" "tmux"
```

Accepts: *list of strings*

| Property | Accepts | Description |
| --- | --- | --- |
| `enablerepo=` (optional) | *string* | A repository that is enabled for this install only. It works with dnf only, so the batch must resolve to `fedora` or `rhel` alone. |

<a id="package-groups"></a>

## `package-groups` (optional, repeatable)

The package groups that this module installs. Package groups work with dnf only, so an ungated `package-groups` needs a module that supports only `fedora` or `rhel`.

```kdl
package-groups "kde-desktop"
```

Accepts: *list of strings*

| Property | Accepts | Description |
| --- | --- | --- |
| `enablerepo=` (optional) | *string* | A repository that is enabled for this install only. |

<a id="copr"></a>

## `copr` (optional, unique)

The `owner/project` name of a COPR repository that this module enables for its own installs. `copr` works on Fedora only.

```kdl
copr "owner/project"
```

Accepts: *string*

<a id="satisfies"></a>

## `satisfies` (optional)

Claims that this module meets rules of a hardening benchmark, such as STIG. The compliance scan checks each claim against the built image, so a false claim shows.

```kdl
satisfies
```

Accepts: optionally {&nbsp;[fields](#satisfies-fields)&nbsp;}

`tect generate` writes every claim into `generated/plan.json`. The compliance job in `.github/workflows/build.yml` then:

- resolves each number to an XCCDF rule ID through the SSG datastream;
- scans the pushed image and compares each claim with the scan result;
- reports a number that maps to no rule as a failed declaration;
- reports a rule that the image fails as a false claim;
- reports a failed rule as a composition failure, if the overlay of another module owns the final copy of a file that this module ships. `plan.json` carries `overlay_overridden` for that case;
- skips a target whose modules declare nothing, and says so;
- reads `.modules[]` only and never `.suppressed[]`, because a suppressed module adds no layer.

<a id="satisfies-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#satisfies-name) (optional,&nbsp;unique) | *list of strings* | One benchmark, which the node name names, with the rule numbers that it covers. The set of benchmarks is open. |

<a id="satisfies-name"></a>

### `<name>` (optional, unique)

One benchmark, which the node name names, with the rule numbers that it covers. The set of benchmarks is open.

```kdl
satisfies {
    stig "RHEL-09-232010"
}
```

Accepts: *list of strings*

<a id="family"></a>

## `family` (optional, repeatable)

Holds the declarations that differ by base family, such as package names that differ between Fedora and Debian. One `module.kdl` can then support several families.

```kdl
family "debian" "ubuntu" {
    packages "htop" "tmux"
}
```

Accepts: *list of strings*, then {&nbsp;[fields](#family-fields)&nbsp;}

<a id="family-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`packages`](#packages) (optional,&nbsp;repeatable) | *list of strings* | The packages that this module installs. |
| [`package-groups`](#package-groups) (optional,&nbsp;repeatable) | *list of strings* | The package groups that this module installs. Package groups work with dnf only, so an ungated `package-groups` needs a module that supports only `fedora` or `rhel`. |
| [`copr`](#copr) (optional,&nbsp;unique) | *string* | The `owner/project` name of a COPR repository that this module enables for its own installs. `copr` works on Fedora only. |
| [`requires`](#requires) (optional,&nbsp;repeatable) | *list of strings* | A capability that another module must provide, which also orders the build. |
| [`after`](#after) (optional,&nbsp;repeatable) | *list of strings* | A capability that this module builds after, which the module does not require. |
| [`satisfies`](#satisfies) (optional) | optionally {&nbsp;[fields](#satisfies-fields)&nbsp;} | An audit declaration of the benchmarks and rules that this module claims to harden. `tect` records the claim and certifies nothing, and the scan checks it after the build. |

> [!NOTE]
> A declaration outside a gate applies to every family the module supports. A declaration inside a gate applies to the families that the gate names.
>
> One gate takes several family names, so the user writes a list that two families share once.
>
> `provides` stays outside a gate. A capability that one family offers and another does not belongs in a second module.
>
> `tect` reads every gate whether or not the image builds for it, so the manifest gets the same checks on each family it claims.
>
> The build drops each gated declaration for a family that it does not build. A Fedora-only `after` does not dangle on a Debian image, and a Debian scan does not map a Fedora rule number.
>
> If a gate names a family that the module does not `supports`, then `tect` refuses the gate.
