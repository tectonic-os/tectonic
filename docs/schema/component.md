<a id="manifest.kdl"></a>

# `manifest.kdl`

Location: `components/<component-name>/manifest.kdl`, in the directory of the component.

A component is one reusable part of an image, such as a desktop, a kernel or a set of tools. `manifest.kdl` declares its schema version, what the component needs and provides, what it installs and which families it targets, and the files beside it do the work.

```kdl
schema-version 1
description "Traditional CLI utilities"
targets "fedora" "debian"
packages "htop" "tmux"
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`schema-version`](#schema-version) (required) | *number* | The schema release that this file is written against. |
| [`description`](#description) (optional) | *string* | One line that names the component in the resolved build summary. |
| [`targets`](#targets) (optional,&nbsp;repeatable) | *list of strings* | The base families that this component builds on, which the image's `family` must match. Omit it to target every family. |
| [`provides`](#provides) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#provides-fields)&nbsp;} | A capability that this component provides for the components that require it. |
| [`requires`](#requires) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#requires-fields)&nbsp;} | A capability that another component must provide, which also orders the build. |
| [`imports`](#imports) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#imports-fields)&nbsp;} | A component that builds in every image that holds this component. |
| [`after`](#after) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#after-fields)&nbsp;} | A capability that this component builds after, which the component does not require. |
| [`removes`](#removes) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#removes-fields)&nbsp;} | A capability that this component withdraws from the image. |
| [`overrides`](#overrides) (optional,&nbsp;repeatable) | *list of strings* | An absolute path that this component replaces on purpose. |
| [`mode`](#mode) (optional,&nbsp;unique) | *path*, then *octal mode* | The octal file mode that one path in the overlay of this component takes. |
| [`key`](#key) (optional,&nbsp;unique) | *string*, then {&nbsp;[fields](#key-fields)&nbsp;} | A key that `tect create key` generates for this component. |
| [`secret`](#secret) (optional,&nbsp;repeatable) | *list of strings* | A build secret that the layer of this component mounts. |
| [`arg`](#arg) (optional,&nbsp;repeatable) | *list of strings* | A build argument that the layer of this component reads. |
| [`helpers`](#helpers) (optional,&nbsp;repeatable) | *list of strings* | Files from this component that every component layer mounts by basename into `/ctx/lib`. |
| [`allow-verify`](#allow-verify) (optional,&nbsp;repeatable) | *string* | One `tect validate-image` diagnostic that the check accepts on one unit, while the check still covers every other diagnostic and unit of the image. |
| [`refuses`](#refuses) (optional,&nbsp;unique) | *string* | One benchmark rule that this component leaves unsatisfied on purpose. |
| [`collects`](#collects) (optional,&nbsp;repeatable) | *string* | A filename that this component gathers from every component that ships one. A collector claims one filename across the image. |
| [`contributes`](#contributes) (optional,&nbsp;repeatable) | *string* | A file that this component ships for another component to collect. |
| [`fragment`](#fragment) (optional) |  | Where the component's `Containerfile.inc` goes in relation to the generated layer. |
| [`option`](#option) (optional,&nbsp;unique) | *string*, then {&nbsp;[fields](#option-fields)&nbsp;} | One value that an image can set on this component. |
| [`variant`](#variant) (optional,&nbsp;unique) | *string*, then optionally {&nbsp;[fields](#variant-fields)&nbsp;} | A named set of option values that an image selects with `variant=`. |
| [`asset`](#asset) (optional,&nbsp;unique) | *string*, then {&nbsp;[fields](#asset-fields)&nbsp;} | A pinned upstream payload that the component fetches during its own layer. |
| [`network`](#network) (optional) | `scripts` | The declaration that the script step of this component reaches the network. |
| [`packages`](#packages) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#packages-fields)&nbsp;} | The packages that this component installs. |
| [`package-groups`](#package-groups) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#package-groups-fields)&nbsp;} | The package groups that this component installs. Package groups work with dnf only, so an ungated `package-groups` needs a component that targets only `fedora` or `rhel`. |
| [`copr`](#copr) (optional,&nbsp;unique) | *string* | The `owner/project` name of a COPR repository that this component enables for its own installs. `copr` works on Fedora only. |
| [`satisfies`](#satisfies) (optional) | optionally {&nbsp;[fields](#satisfies-fields)&nbsp;} | An audit declaration of the benchmarks and rules that this component claims to harden. `tect` records the claim and certifies nothing, and the scan checks it after the build. |
| [`on`](#on) (optional,&nbsp;repeatable) | *list of strings*, then optionally {&nbsp;[fields](#on-fields)&nbsp;} | The declarations that the build takes only on the named base families. |
| [`base`](#base) (optional) | *string*, then optionally {&nbsp;[fields](#base-fields)&nbsp;} | The image reference that an image on this base builds from. It makes the component a base. |

> [!NOTE]
> `requires` and `after` decide the build order. The order of the image list only breaks ties.
>
> If nothing provides a `requires`, then `tect check` fails and names every component that would satisfy it.

<a id="schema-version"></a>

## `schema-version` (required)

The schema version tells `tect` which reader to use for this file. A file written against an earlier release keeps using that release's reader, while `repo.kdl` sets the newest schema the repository accepts.

```kdl
schema-version 1
```

Accepts: *number*

> [!NOTE]
> A file cannot declare a schema newer than `repo.kdl`.
>
> `tect create repo`, `tect create image` and `tect create component` write the version, and `tect` never changes it.

<a id="description"></a>

## `description` (optional)

One line that names the component in the resolved build summary.

```kdl
description "Traditional CLI utilities"
```

Accepts: *string*

<a id="targets"></a>

## `targets` (optional, repeatable)

Lists the base families that the component works on. `tect check` refuses the component on an image whose base family is not in the list, so a portability gap shows before the build. A component with no `targets` declaration is base-agnostic and targets every family.

```kdl
targets "fedora" "debian"
```

Accepts: *list of strings*

<a id="provides"></a>

## `provides` (optional, repeatable)

Names a capability that this component adds to the image, such as `flatpak`. Another component `requires` it by name. If a name declares `witness`, then the build checks the finished image for that file. A name may have `/`-separated segments, and `rpm/dnf5` also meets a requirement for `rpm`. Several names go on one line, as `provides "a" "b"`, or in a block with one name on each line, where each name carries its own properties.

```kdl
provides "flatpak"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#provides-fields)&nbsp;}

| Property | Accepts | Description |
| --- | --- | --- |
| `witness=` (optional) | *string* | The absolute path in the finished image that witnesses the capability. Without it, nothing witnesses the capability. |
| `build-only=` (optional) | *boolean* | Whether the `witness` exists only while the build runs. |
| `exclusive=` (optional) | *boolean* | Whether a second enabled provider of the capability conflicts with this one. |

<a id="provides-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#provides-name) (optional,&nbsp;repeatable) |  | One capability in a `provides` block, which the node name names. |

> [!NOTE]
> The build checks the finished image for the witness file.
>
> A `provides` without `witness`, such as `rechunking`, has no witness. `tect` holds no default path for a name.
>
> `tect validate-image` checks the finished image for the witness of each name that the base or a component provides with a `witness`. `base-sig-probe` reads those witnesses when it measures a base, except the ones of a component that meets a base requirement or that the base imports.
>
> The witness of `luks-initramfs` is inside an archive. If a target declares it, then the build validates the initramfs with `lsinitrd`. If no target declares it, then the installer does not offer root encryption.
>
> Declare `luks-initramfs` in a custom base or component only if its initramfs unlocks LUKS. `tect build` then proves that the binary is present before the installer offers it.
>
> Many enabled components may provide one capability. If one of them declares it `exclusive=#true`, then `tect check` refuses a second provider that builds into the same image or flavour. `tect import component` asks whether to replace the first. Providers in sibling flavours never build together.
>
> A capability name is lower-case letters, digits and dashes, and starts with a letter. The generated graph writes the name unquoted into a mermaid label and a markdown table cell.

<a id="provides-name"></a>

### `<name>` (optional, repeatable)

One capability in a `provides` block, which the node name names.

```kdl
provides {
    "ssh" witness="/usr/sbin/sshd"
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `witness=` (optional) | *string* | The absolute path in the finished image that witnesses the capability. Without it, nothing witnesses the capability. |
| `build-only=` (optional) | *boolean* | Whether the `witness` exists only while the build runs. |
| `exclusive=` (optional) | *boolean* | Whether a second enabled provider of the capability conflicts with this one. |

<a id="requires"></a>

## `requires` (optional, repeatable)

Names a capability that the image must have for this component to work. A component meets it by its own path, or by a `provides` name equal to it or below it, so `rpm/dnf5` meets `rpm`. The base, a component that the image lists or a component that an `imports` brings must meet it. If none does, then `tect check` fails, and `tect import component` offers the providers. `tect` builds this component after each component that meets it.

```kdl
requires "kernel-devel"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#requires-fields)&nbsp;}

<a id="requires-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#requires-name) (optional,&nbsp;repeatable) |  | One capability in the block, which the node name names. |

<a id="requires-name"></a>

### `<name>` (optional, repeatable)

One capability in the block, which the node name names.

```kdl
requires {
    "kernel-devel"
}
```

<a id="imports"></a>

## `imports` (optional, repeatable)

Names a component under `components/` by its path. The imported component builds in every image that holds this component, so the image file does not list it. An image entry at an imported path fails `tect check`. A path may read an option of this component as `$<name>`, so the image chooses the component in this component's block. Several paths go on one line, as `imports "a" "b"`, or in a block with one path on each line. In the block, a path's own block sets the imported component's options. A value may read this component's options the same way, and an option that the block does not set takes its default. If two components import one component with different values for one option, then `tect check` fails.

```kdl
imports "container-runtime"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#imports-fields)&nbsp;}

<a id="imports-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#imports-name) (optional,&nbsp;repeatable) | optionally {&nbsp;[fields](#imports-name-fields)&nbsp;} | One component path in an `imports` block, which the node name names. Its block sets the options of that component. |

<a id="imports-name"></a>

### `<name>` (optional, repeatable)

One component path in an `imports` block, which the node name names. Its block sets the options of that component.

```kdl
imports {
    "container-runtime"
}
```

Accepts: optionally {&nbsp;[fields](#imports-name-fields)&nbsp;}

<a id="imports-name-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#imports-name-name) (optional,&nbsp;repeatable) |  | One option value that the imported component takes, which the node name names. |

<a id="imports-name-name"></a>

#### `<name>` (optional, repeatable)

One option value that the imported component takes, which the node name names.

```kdl
imports {
    "container-runtime" {
        storage-driver "$storage-driver"
    }
}
```

<a id="after"></a>

## `after` (optional, repeatable)

Names a capability that this component builds after. If an enabled component provides it, then `tect` builds this component after that component. If no enabled component provides it, then the `after` orders nothing.

```kdl
after "vfio"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#after-fields)&nbsp;}

<a id="after-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#requires-name) (optional,&nbsp;repeatable) |  | One capability in the block, which the node name names. |

<a id="removes"></a>

## `removes` (optional, repeatable)

Names a capability that the base or another component provides and that this component takes away, such as a tool that its script uninstalls. The image then no longer claims the capability. `tect` builds this component after every provider of the name. `tect check` refuses a `removes` that nothing in the image provides, and it refuses every component that requires a removed name. `tect validate-image` checks that the witness of a removed name is absent from the finished image.

```kdl
removes "bootupctl"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#removes-fields)&nbsp;}

<a id="removes-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#requires-name) (optional,&nbsp;repeatable) |  | One capability in the block, which the node name names. |

<a id="overrides"></a>

## `overrides` (optional, repeatable)

Declares that this component replaces a file that an earlier component also ships. Without it, `tect check` reports the two overlays as a collision. `tect check` also reports an override that no earlier component collides with, so an override cannot outlive its reason.

```kdl
overrides "/etc/containers/policy.json"
```

Accepts: *list of strings*

<a id="mode"></a>

## `mode` (optional, unique)

Sets the file mode of one file that the `files/` overlay of the component installs, such as `0600` for a file that only root reads.

```kdl
mode "/etc/audit/rules.d/audit.rules" "0600"
```

Accepts: *path*, then *octal mode*

<a id="key"></a>

## `key` (optional, unique)

A component that needs a key declares it here, such as a Secure Boot signing key. `tect create key` generates the key from this declaration. The build installs the public half into the image at the path that `public` names. The private half stays in `keys/private/`, outside the image and outside git.

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
| [`public`](#key-public) (required) | *string* | Where the public half ships, which witnesses the `<kind>-key` capability of this component. |
| [`private`](#key-private) (required) | *string* | The filename that the private half takes under `keys/private/`. |

<a id="key-generator"></a>

### `generator` (required)

`tect create key` offers every key that a component declares. `generator` names the tool that writes this key, and its properties set the key up for what the component uses it for.

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

Where the build installs the public half in the image. The file also witnesses the `<kind>-key` capability, so another component can `requires` the key.

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

Names a build secret that the layer of this component reads, such as a signing key. The layer mounts it at `/run/secrets/<name>`, so the secret is there while the component builds and never lands in the image.

```kdl
secret "mok_privkey"
```

Accepts: *list of strings*

<a id="arg"></a>

## `arg` (optional, repeatable)

Names a build argument that the layer of this component reads as an environment variable, such as the kernel version to install.

```kdl
arg "KERNEL"
```

Accepts: *list of strings*

<a id="helpers"></a>

## `helpers` (optional, repeatable)

Shares shell helpers from this component with every other component. The build mounts each listed file by its basename into `/ctx/lib` in every component layer, so another component can source it.

```kdl
helpers "lib/family.sh"
```

Accepts: *list of strings*

<a id="allow-verify"></a>

## `allow-verify` (optional, repeatable)

`tect validate-image` checks the systemd units of the finished image. If one unit of this component raises an expected diagnostic, then this accepts that one diagnostic on that one unit. The check still covers every other diagnostic and unit.

```kdl
allow-verify "man-page-missing" unit="plasmalogin.service"
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `unit=` (optional) | *string* | The unit the exception applies to. |

<a id="refuses"></a>

## `refuses` (optional, unique)

A hardening rule can break what a component does. This records that the component leaves one rule unsatisfied on purpose, and `because=` says why. Remediation then does not set the rule, and the choice is visible to every image.

```kdl
refuses "grub2_nousb_argument" because="it removes the keyboard"
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `because=` (optional) | *string* | Why the component leaves the rule unsatisfied. |

> [!NOTE]
> Remediation does not set a refused rule. If an image declares `allow-remediation` for the rule, then remediation can set it.

<a id="collects"></a>

## `collects` (optional, repeatable)

Builds one file from parts that many components ship, such as a list of Flatpak apps. This component claims the file name, every component that ships a file of that name contributes a part, and the finalize phase assembles the parts at `into=`.

```kdl
collects "flatpaks.list" into="/usr/share/flatpak-defaults/apps.list" priority=500
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `into=` (optional) | *string* | The absolute path that receives the assembled file. |
| `priority=` (optional) | *number* from 0 to 9999 | The position of a contribution that declares no priority. |

> [!NOTE]
> The build stages each contribution as `<into>.d/NNNN-<component>.part`. The finalize phase assembles the parts in that order, so the build order of the contributors does not change the assembled file.
>
> A component ships at most one copy of a collected file. The most specific copy wins. The order is `debian/`, then `deb/`, then the component root.

<a id="contributes"></a>

## `contributes` (optional, repeatable)

A file that this component ships for another component to collect.

```kdl
contributes "flatpaks.list"
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `priority=` (optional) | *number* from 0 to 9999 | The position that this file takes in the assembled file. |

<a id="fragment"></a>

## `fragment` (optional)

A component whose needs the fields cannot express ships a `Containerfile.inc`, which the build adds verbatim. This places those lines relative to the generated layer.

```kdl
fragment position="after"
```

| Property | Accepts | Description |
| --- | --- | --- |
| `position=` (optional) | `before` or `after` or `tail` | Where the fragment goes. `before`, the default, puts it above the generated block. `after` puts it below the block. `tail` puts it below the finalize layer, where the lineage stage has ended and can be named. |
| `standard-layer=` (optional) | *boolean* | Whether the build emits the generated block. |

> [!NOTE]
> The build places the fragment verbatim and replaces `@COMPONENT@` with `/components/<dir>`, the component directory in the build context. The mounts of the component layer use the same path, so a `COPY --from=ctx` needs it to reach a file that the component ships.
>
> Every `RUN` bind-mounts over `/etc/hostname`, `/etc/hosts` and `/etc/resolv.conf`. A fragment writes those three files with a `COPY --from=ctx`.

<a id="option"></a>

## `option` (optional, unique)

Lets an image change how this component builds, such as which fonts it installs. The component declares the option with a type and a default, and each image can set its own value.

```kdl
option "version" type="string" {
    default "128.0"
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
| [`default`](#option-default) (required) | *list of strings* | The value that the component builds with if no image sets one. |
| [`renovate`](#option-renovate) (optional) |  | What keeps the value of a `version` option current. |

> [!NOTE]
> Every declared option reaches the layer of its component as `OPT_<NAME>`. `<NAME>` is the option name in upper case, and each dash becomes an underscore.
>
> A default reaches the layer too, so `component.sh` reads a variable and does not test whether one is set.
>
> A `string` option's value fills each `$<name>` that names it in a `packages`, `package-groups` or `provider` argument, as `packages "firefox=$version"` does. A `$<name>` that names a `bool` or a `list` option, or no option, fails `tect check`.

<a id="option-description"></a>

### `description` (optional)

What the option does.

```kdl
option "version" type="string" {
    description "The Firefox release to install"
}
```

Accepts: *string*

<a id="option-default"></a>

### `default` (required)

The value that the component builds with if no image sets one.

```kdl
option "version" type="string" {
    default "128.0"
}
```

Accepts: *list of strings*

<a id="option-renovate"></a>

### `renovate` (optional)

Names where Renovate looks up new versions of the software that a `version` option installs. `tect generate` writes one Renovate manager for each image entry that sets the option, so Renovate updates the value in the image file and the component stays as it is.

```kdl
option "version" type="string" {
    renovate datasource="repology" depName="fedora_rawhide/firefox"
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `datasource=` (required) | *string* | The Renovate datasource that publishes the versions. |
| `depName=` (required) | *string* | The name that the datasource knows the software by. |
| `extractVersion=` (optional) | *string* | The pattern that extracts the version from what the datasource reports. |

<a id="variant"></a>

## `variant` (optional, unique)

Bundles option values under one name, so an image takes a whole configuration of the component with `variant=` in place of setting each option.

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

Downloads a pinned file from upstream, such as a release binary, while this component builds. The pin fixes the version and the hash, so every build gets the same bytes and Renovate can keep the version current.

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

The layer of the component gets three variables, where `<NAME>` is the asset name in upper case with each dash turned into an underscore:

- `ASSET_<NAME>_VERSION`;
- `ASSET_<NAME>_URL`, with `{version}` already expanded, so no shell code builds a URL;
- `ASSET_<NAME>_SHA256`.

<a id="asset-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`pin`](#asset-pin) (optional) | {&nbsp;[fields](#asset-pin-fields)&nbsp;} | Where content comes from, which version it is, what verifies it, and what keeps the version current. |

<a id="asset-pin"></a>

### `pin` (optional)

A pin fixes where a download comes from and which version it is, and says how the version stays current. `tect` verifies each download against the pinned hash, so a changed upstream file is caught.

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
- `manual`, if nothing tracks the pin.

<a id="asset-pin-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`renovate`](#asset-pin-renovate) (optional) |  | The Renovate custom manager that keeps `version` current. |
| [`manual`](#asset-pin-manual) (optional) | *string* | Why nothing tracks this pin. |
| [`version`](#asset-pin-version) (required) | *string* | The version, tag or commit that `url` expands and Renovate rewrites. |
| [`url`](#asset-pin-url) (required) | *string* | Where the content comes from. |
| [`sha256`](#asset-pin-sha256) (optional) | *string* | The hash the fetched content must match. |

> [!NOTE]
> An `asset` holds a `pin`. A library in `sources` declares its own `version`, `sha256` and `unpinned`.
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
> If a pin names a download and no `sha256`, then `tect check` reports an error. A pin whose `url` is a Git repository is cloned at its `version` and declares none.

<a id="network"></a>

## `network` (optional)

Declares that the scripts of this component need the network. A repository with a `strict` script rule opens the network only for the components that declare it.

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

Lists the distribution packages that this component installs with the package manager of the base family. A name can carry a version after one `=`, such as `firefox=$version`. The build spells it `firefox-<version>` for dnf and `firefox=<version>` for apt. Several names go on one line, as `packages "htop" "tmux"`, or in a block with one name on each line. Each `packages` node installs as one batch, so `enablerepo` sits on the node and covers every name in it.

```kdl
packages "htop" "tmux"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#packages-fields)&nbsp;}

| Property | Accepts | Description |
| --- | --- | --- |
| `enablerepo=` (optional) | *string* | A repository that is enabled for this install only. It works with dnf only, so the batch must resolve to `fedora` or `rhel` alone. |

<a id="packages-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#packages-name) (optional,&nbsp;repeatable) |  | One name in the block, which the node name names. |

<a id="packages-name"></a>

### `<name>` (optional, repeatable)

One name in the block, which the node name names.

```kdl
packages {
    "htop"
}
```

<a id="package-groups"></a>

## `package-groups` (optional, repeatable)

The package groups that this component installs. Package groups work with dnf only, so an ungated `package-groups` needs a component that targets only `fedora` or `rhel`.

```kdl
package-groups "kde-desktop"
```

Accepts: *list of strings*, or {&nbsp;[one name per line](#package-groups-fields)&nbsp;}

| Property | Accepts | Description |
| --- | --- | --- |
| `enablerepo=` (optional) | *string* | A repository that is enabled for this install only. |

<a id="package-groups-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#packages-name) (optional,&nbsp;repeatable) |  | One name in the block, which the node name names. |

<a id="copr"></a>

## `copr` (optional, unique)

The `owner/project` name of a COPR repository that this component enables for its own installs. `copr` works on Fedora only.

```kdl
copr "owner/project"
```

Accepts: *string*

<a id="satisfies"></a>

## `satisfies` (optional)

Claims that this component meets rules of a hardening benchmark, such as STIG. The compliance scan checks each claim against the built image, so a false claim shows.

```kdl
satisfies
```

Accepts: optionally {&nbsp;[fields](#satisfies-fields)&nbsp;}

`tect generate` writes every claim into `generated/plan.json`. The compliance job in `.github/workflows/build.yml` then:

- resolves each number to an XCCDF rule ID through the SSG datastream;
- scans the pushed image and compares each claim with the scan result;
- reports a number that maps to no rule as a failed declaration;
- reports a rule that the image fails as a false claim;
- reports a failed rule as a composition failure, if the overlay of another component owns the final copy of a file that this component ships. `plan.json` carries `overlay_overridden` for that case;
- skips a target whose components declare nothing, and says so;
- reads `.components[]` only and never `.suppressed[]`, because a suppressed component adds no layer.

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

<a id="on"></a>

## `on` (optional, repeatable)

Holds the declarations that differ by base family, such as package names that differ between Fedora and Debian. One `manifest.kdl` can then target several families.

```kdl
on "debian" "ubuntu"
```

Accepts: *list of strings*, then optionally {&nbsp;[fields](#on-fields)&nbsp;}

<a id="on-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`packages`](#packages) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#packages-fields)&nbsp;} | The packages that this component installs. |
| [`package-groups`](#package-groups) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#package-groups-fields)&nbsp;} | The package groups that this component installs. Package groups work with dnf only, so an ungated `package-groups` needs a component that targets only `fedora` or `rhel`. |
| [`copr`](#copr) (optional,&nbsp;unique) | *string* | The `owner/project` name of a COPR repository that this component enables for its own installs. `copr` works on Fedora only. |
| [`requires`](#requires) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#requires-fields)&nbsp;} | A capability that another component must provide, which also orders the build. |
| [`imports`](#imports) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#imports-fields)&nbsp;} | A component that builds in every image that holds this component. |
| [`after`](#after) (optional,&nbsp;repeatable) | *list of strings*, or {&nbsp;[one name per line](#after-fields)&nbsp;} | A capability that this component builds after, which the component does not require. |
| [`satisfies`](#satisfies) (optional) | optionally {&nbsp;[fields](#satisfies-fields)&nbsp;} | An audit declaration of the benchmarks and rules that this component claims to harden. `tect` records the claim and certifies nothing, and the scan checks it after the build. |

> [!NOTE]
> A declaration outside a gate applies to every family the component targets. A declaration inside a gate applies to the families that the gate names.
>
> One gate takes several family names, so the user writes a list that two families share once.
>
> `provides` stays outside a gate. A capability that one family offers and another does not belongs in a second component.
>
> `tect` reads every gate whether or not the image builds for it, so the manifest gets the same checks on each family it claims.
>
> The build drops each gated declaration for a family that it does not build. A Fedora-only `after` does not order the component after a Debian provider, and a Debian scan does not map a Fedora rule number.
>
> If a gate names a family that the component does not target, then `tect` refuses the gate.

<a id="base"></a>

## `base` (optional)

Names the upstream bootc image that a base stands for, and makes the component a base. A base provides `base-image` exclusively, so an image holds one. The reference usually reads the base's `version` option, as `base "quay.io/fedora/fedora-bootc:$version"` does, so the image sets the version on the base's entry. A block after the reference holds what the base states about itself. A base with no block is valid, and each field it leaves out takes the effect its own row states.

```kdl
base "quay.io/fedora/fedora-bootc:$version"
```

Accepts: *string*, then optionally {&nbsp;[fields](#base-fields)&nbsp;}

<a id="base-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`family`](#base-family) (optional) | *string* | The family that an image on this base installs as. |
| [`bootloader`](#base-bootloader) (optional) | *list of strings* | The bootloaders that an image on this base can install, with the default first. |
| [`scap-content`](#base-scap-content) (optional) | *string* | The SSG datastream file that a scan of an image on this base reads. |

<a id="base-family"></a>

### `family` (optional)

Names the base family, such as `fedora` or `debian`, which picks the package manager and the family's defaults. If a base names no family, then `tect check` holds no component to its `targets` on that base.

```kdl
base "quay.io/fedora/fedora-bootc:$version" {
    family "fedora"
}
```

Accepts: *string*

<a id="base-bootloader"></a>

### `bootloader` (optional)

Lists the bootloaders that the base carries. `tect create image` offers them when it writes the image's `layout`. If a base names none, then `tect create image` chooses no bootloader, and the user names one in the `layout`.

```kdl
base "quay.io/fedora/fedora-bootc:$version" {
    bootloader "grub2"
}
```

Accepts: *list of strings*

<a id="base-scap-content"></a>

### `scap-content` (optional)

Names the SCAP content file that measures an image on this base, as a filename in the content directory. If a base names no content, then the scan reads the default content of the base's family. Each EL release publishes its own datastream, so the `rhel` family has no default. `tect check` refuses an image that conforms to a benchmark while its base resolves to no content.

```kdl
base "quay.io/fedora/fedora-bootc:$version" {
    scap-content "ssg-fedora-ds.xml"
}
```

Accepts: *string*
