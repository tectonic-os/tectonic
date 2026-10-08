<a id="image.kdl"></a>

# `image.kdl`

Location: The root of the repository, as `image.kdl` or `<name>.image.kdl`.

An image file holds its schema version and one or more images. Each image says what it is called, which base it builds on and which modules it is made of.

```kdl
schema-version 1
image {
    name "Workstation"
    url "https://github.com/owner/workstation"
    issues-url "https://github.com/owner/workstation/issues"
    base "quay.io/fedora/fedora-bootc:44" {
        family "fedora"
        provides "rechunking" "bootc"
        signed #true
    }
    layout {
        bootloader "grub2"
    }
    modules {
        module "core/bootloader"
    }
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`schema-version`](#schema-version) (required) | *number* | The schema release that this file is written against. |
| [`image`](#image) (required,&nbsp;repeatable) | {&nbsp;[fields](#image-fields)&nbsp;} | One image, which declares its names, its base and every module that it is made of. |

> [!NOTE]
> A root `.kdl` file is an image file only if it is named `image.kdl` or ends in `.image.kdl`. `tect` reports each other root `.kdl` file and does not read it.
>
> The part of the file name in front of `.image.kdl` does not name the image. An image is called what it declares, so one file can hold as many images as suit the repository. `plan.json` records the file name only to say where each image is declared.

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
> `tect create repo`, `tect create image` and `tect create module` write the version, and `tect` never changes it.

<a id="image"></a>

## `image` (required, repeatable)

Declares one bootable operating system image: its names, the base it builds on, how the installer lays it down and the modules that make it up. The build publishes the image under its `id`.

```kdl
image {
    name "Workstation"
    base "quay.io/fedora/fedora-bootc:44"
    modules {
        module "core/bootloader"
    }
}
```

Accepts: {&nbsp;[fields](#image-fields)&nbsp;}

<a id="image-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`id`](#image-id) (optional) | *string* | os-release `DEFAULT_HOSTNAME`. The machine name of the image. |
| [`name`](#image-name) (required) | *string* | os-release `NAME`. The name that the boot menu and the desktop read. |
| [`pretty-name`](#image-pretty-name) (optional) | *string* | os-release `PRETTY_NAME`. The full name that the user sees. |
| [`url`](#image-url) (optional) | *string* | os-release `HOME_URL` and `DOCUMENTATION_URL`. The URL that serves the home page of the project. |
| [`issues-url`](#image-issues-url) (optional) | *string* | os-release `SUPPORT_URL` and `BUG_REPORT_URL`. Where the user reports a problem with the image. |
| [`description`](#image-description) (optional) | *string* | OCI label `org.opencontainers.image.description`. One line that summarises the image. |
| [`keywords`](#image-keywords) (optional,&nbsp;repeatable) | *list of strings* | OCI label `io.artifacthub.package.keywords`. The keywords of the image, which the build joins with commas. |
| [`logo-url`](#image-logo-url) (optional) | *string* | OCI label `io.artifacthub.package.logo-url`. The URL that points at the logo of the image. |
| [`conforms`](#image-conforms) (optional) | *string* | The benchmark profile that a scan measures the ungated target against. |
| [`boot`](#image-boot) (optional) | `uki-shim` or `uki-db` | The UKI boot chain, which is owner-signed through shim or enrolled directly into Secure Boot. |
| [`base`](#image-base) (required) | *string*, then optionally {&nbsp;[fields](#image-base-fields)&nbsp;} | The Linux image that every layer builds on. Its block declares what a build on the base can assume. |
| [`layout`](#image-layout) (optional) | {&nbsp;[fields](#image-layout-fields)&nbsp;} | What the installer lays down if the answer of the base family is not the one that the image wants. |
| [`allow-remediation`](#image-allow-remediation) (optional,&nbsp;unique) | *string* | One rule an installed module refuses that this image lets remediation set anyway. |
| [`flavours`](#image-flavours) (optional) | {&nbsp;[fields](#image-flavours-fields)&nbsp;} | The flavours that this image publishes beside its ungated build. |
| [`modules`](#image-modules) (required) | {&nbsp;[fields](#image-modules-fields)&nbsp;} | Every module that the image is made of, whether a flavour gates it or not. |

<a id="image-id"></a>

### `id` (optional)

os-release `DEFAULT_HOSTNAME`. The machine name of the image.

```kdl
image {
    id "workstation"
}
```

Accepts: *string*

The `id` names:

- the published image;
- the build target;
- the cache tag;
- the os-release `DEFAULT_HOSTNAME`.

> [!NOTE]
> If `id` is absent, then `tect` derives it from `name` in lower case and turns each space into a dash.

<a id="image-name"></a>

### `name` (required)

os-release `NAME`. The name that the boot menu and the desktop read.

```kdl
image {
    name "Workstation"
}
```

Accepts: *string*

> [!NOTE]
> If `pretty-name` is absent, then `PRETTY_NAME` is `name` followed by the image version.

<a id="image-pretty-name"></a>

### `pretty-name` (optional)

os-release `PRETTY_NAME`. The full name that the user sees.

```kdl
image {
    pretty-name "Workstation 44"
}
```

Accepts: *string*

<a id="image-url"></a>

### `url` (optional)

os-release `HOME_URL` and `DOCUMENTATION_URL`. The URL that serves the home page of the project.

```kdl
image {
    url "https://github.com/owner/workstation"
}
```

Accepts: *string*

<a id="image-issues-url"></a>

### `issues-url` (optional)

os-release `SUPPORT_URL` and `BUG_REPORT_URL`. Where the user reports a problem with the image.

```kdl
image {
    issues-url "https://github.com/owner/workstation/issues"
}
```

Accepts: *string*

<a id="image-description"></a>

### `description` (optional)

OCI label `org.opencontainers.image.description`. One line that summarises the image.

```kdl
image {
    description "A KDE desktop for daily work"
}
```

Accepts: *string*

<a id="image-keywords"></a>

### `keywords` (optional, repeatable)

OCI label `io.artifacthub.package.keywords`. The keywords of the image, which the build joins with commas.

```kdl
image {
    keywords "desktop" "kde"
}
```

Accepts: *list of strings*

<a id="image-logo-url"></a>

### `logo-url` (optional)

OCI label `io.artifacthub.package.logo-url`. The URL that points at the logo of the image.

```kdl
image {
    logo-url "https://github.com/owner/workstation/raw/main/logo.svg"
}
```

Accepts: *string*

<a id="image-conforms"></a>

### `conforms` (optional)

Names the benchmark profile that a scan measures the image against. Setting it turns the scan on: every build measures the image against the profile and publishes the score.

```kdl
image {
    conforms "standard"
}
```

Accepts: *string*

> [!NOTE]
> A scan reports the result. A failed rule fails `tect scap` only if `audit { enforce #true }` is set.

<a id="image-boot"></a>

### `boot` (optional)

Builds the image to boot as a unified kernel image (UKI) under Secure Boot. `uki-shim` boots through shim with a key the owner signs, and `uki-db` enrolls the key directly into the firmware key database.

```kdl
image {
    boot "uki-shim"
}
```

Accepts: `uki-shim` or `uki-db`

<a id="image-base"></a>

### `base` (required)

The base is the upstream bootc image that the custom image starts from. Its block tells `tect` what the base already has, so `tect` checks each module against it and skips a module that the base already covers.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44"
}
```

Accepts: *string*, then optionally {&nbsp;[fields](#image-base-fields)&nbsp;}

The reference can carry a digest after its tag:

- `tect` strips the digest before it looks up the catalog, so a pinned base is still the base of the catalog.
- `tect create image --base` writes the same `family`, `provides` and `requires` for a tag with a digest and for a tag without one. Renovate bumps the half that the image file writes.
- `tect build` records a declared digest verbatim. It records a bare tag as the digest that the registry answers with.
- If the base is `signed #false`, then the digest is the strongest trust root on offer.

<a id="image-base-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`family`](#image-base-family) (optional) | *string* | The optional family used for family-specific module content and compatibility checks. |
| [`provides`](#image-base-provides) (optional,&nbsp;repeatable) | *list of strings* | The capabilities that the upstream image already ships, which the build checks the finished image for. |
| [`requires`](#image-base-requires) (optional,&nbsp;repeatable) | *list of strings* | The capabilities that an enabled module must provide before the base is usable. |
| [`satisfies`](#image-base-satisfies) (optional) | optionally {&nbsp;[fields](#image-base-satisfies-fields)&nbsp;} | An audit declaration of the benchmarks and rules that the base image already satisfies. `tect` records it and certifies nothing. |
| [`signed`](#image-base-signed) (optional) | *boolean* | Whether the base publishes a cosign signature. |

> [!NOTE]
> The build record says what one build got. Only a digest in the image file says what the next build gets.

<a id="image-base-family"></a>

#### `family` (optional)

The family, such as `fedora` or `debian`, selects family-specific module content and is checked against every module that restricts its supported families. When it is omitted, `tect` takes only ungated module content and skips family compatibility checks.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44" {
        family "fedora"
    }
}
```

Accepts: *string*

<a id="image-base-provides"></a>

#### `provides` (optional, repeatable)

Lists what the base image already ships, such as `bootc`. `tect` skips a module that provides only what the base already has, and the build checks the finished image for each name.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44" {
        provides "rechunking" "bootc"
    }
}
```

Accepts: *list of strings*

If the base provides every capability that a listed module provides, then `tect` suppresses the module:

- The module is not ordered or built, and nothing it ships reaches the image.
- The generated graph lists the module, and `plan.json` carries it beside the modules that did build.
- The options and variants of the module still resolve. `tect` checks a value set on one and puts it in the plan, but the value reaches no layer.

> [!NOTE]
> A module that the base covers only in part still builds. If that module declares a capability that the base already provides, then `tect` reports an error.

<a id="image-base-requires"></a>

#### `requires` (optional, repeatable)

The capabilities that an enabled module must provide before the base is usable.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44" {
        requires "bootc-base"
    }
}
```

Accepts: *list of strings*

<a id="image-base-satisfies"></a>

#### `satisfies` (optional)

An audit declaration of the benchmarks and rules that the base image already satisfies. `tect` records it and certifies nothing.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44" {
        satisfies
    }
}
```

Accepts: optionally {&nbsp;[fields](#image-base-satisfies-fields)&nbsp;}

<a id="image-base-satisfies-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#image-base-satisfies-name) (optional,&nbsp;unique) | *list of strings* | One benchmark, which the node name names, with the rule numbers that it covers. The set of benchmarks is open. |

<a id="image-base-satisfies-name"></a>

##### `<name>` (optional, unique)

One benchmark, which the node name names, with the rule numbers that it covers. The set of benchmarks is open.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44" {
        satisfies {
            stig "RHEL-09-232010"
        }
    }
}
```

Accepts: *list of strings*

<a id="image-base-signed"></a>

#### `signed` (optional)

Records whether the publisher of the base signs it with cosign. The build does not verify the signature. The `base-sig-probe` workflow keeps the value current.

```kdl
image {
    base "quay.io/fedora/fedora-bootc:44" {
        signed #true
    }
}
```

Accepts: *boolean*

<a id="image-layout"></a>

### `layout` (optional)

Each base family comes with installer defaults for the filesystem, composefs, the generic image and the administrator group. This block replaces those defaults for this image, so the installer lays the image down the way it needs. No family supplies a bootloader, so a declared `layout` names `bootloader`.

```kdl
image {
    layout {
        filesystem "btrfs"
    }
}
```

Accepts: {&nbsp;[fields](#image-layout-fields)&nbsp;}

<a id="image-layout-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`filesystem`](#image-layout-filesystem) (optional) | `xfs` or `ext4` or `btrfs` | The root filesystem that the installer formats in place of the one that the base family settles. |
| [`composefs`](#image-layout-composefs) (optional) | *boolean* | Whether the install seals the deployment, which replaces the answer of the base family. |
| [`generic-image`](#image-layout-generic-image) (optional) | *boolean* | Whether the install skips the bootupd check, which replaces the answer of the base family. |
| [`admin-group`](#image-layout-admin-group) (optional) | *string* | The group that the installer creates an administrator in, which replaces the group of the base family. |
| [`bootloader`](#image-layout-bootloader) (optional) | `grub2` or `systemd` | The bootloader that the installer installs. The catalog entry of the base lists the bootloaders that the base carries. |

<a id="image-layout-filesystem"></a>

#### `filesystem` (optional)

The root filesystem that the installer formats in place of the one that the base family settles.

```kdl
image {
    layout {
        filesystem "btrfs"
    }
}
```

Accepts: `xfs` or `ext4` or `btrfs`

<a id="image-layout-composefs"></a>

#### `composefs` (optional)

Whether the install seals the deployment, which replaces the answer of the base family.

```kdl
image {
    layout {
        composefs #true
    }
}
```

Accepts: *boolean*

<a id="image-layout-generic-image"></a>

#### `generic-image` (optional)

Whether the install skips the bootupd check, which replaces the answer of the base family.

```kdl
image {
    layout {
        generic-image #false
    }
}
```

Accepts: *boolean*

<a id="image-layout-admin-group"></a>

#### `admin-group` (optional)

The group that the installer creates an administrator in, which replaces the group of the base family.

```kdl
image {
    layout {
        admin-group "wheel"
    }
}
```

Accepts: *string*

<a id="image-layout-bootloader"></a>

#### `bootloader` (optional)

The bootloader that the installer writes to the disk. No base family supplies a default, so `tect check` fails a `layout` without one. If `boot` is set, then the bootloader is `systemd`. If neither is set, then the image builds no install media.

```kdl
image {
    layout {
        bootloader "grub2"
    }
}
```

Accepts: `grub2` or `systemd`

<a id="image-allow-remediation"></a>

### `allow-remediation` (optional, unique)

A module can refuse a hardening rule because the rule breaks what the module does. This lets one image accept that rule anyway, for a machine where the reason of the module does not apply. `because=` records why.

```kdl
image {
    allow-remediation "grub2_nousb_argument" because="this machine has no USB keyboard"
}
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `because=` (optional) | *string* | Why this image overrides the module's judgement. |

<a id="image-flavours"></a>

### `flavours` (optional)

A flavour builds a variant of the same image with extra modules, such as a `dev` build with developer tools, and publishes it as `<id>-<flavour>`. Flavours suit two builds that share almost every module.

```kdl
image {
    flavours {
        dev
    }
}
```

Accepts: {&nbsp;[fields](#image-flavours-fields)&nbsp;}

<a id="image-flavours-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#image-flavours-name) (optional,&nbsp;repeatable) |  | One flavour that publishes a gated module set as `<id>-<flavour>`, where `<flavour>` is the node name. |

> [!NOTE]
> An image builds one target for its ungated module set and one more for each flavour. The targets publish as `<id>` and `<id>-<flavour>`.
>
> A flavour gates modules inside one image. A module listed inside `flavour "dev"` builds only for that target, and each other module builds for every target.
>
> The `conforms=` of a flavour measures that flavour. The `conforms` of the image measures the ungated target only.

<a id="image-flavours-name"></a>

#### `<name>` (optional, repeatable)

One flavour that publishes a gated module set as `<id>-<flavour>`, where `<flavour>` is the node name.

```kdl
image {
    flavours {
        dev
    }
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `default=` (optional) | *boolean* | Whether a build that names no flavour builds this one. |
| `pr-build=` (optional) | *boolean* | Whether a pull request builds this flavour in place of the default. |
| `conforms=` (optional) | *string* | The benchmark profile that a scan measures this flavour against. |

<a id="image-modules"></a>

### `modules` (required)

Lists every module that the image is made of. A module comes from `modules/` in this repository, from a collection in `sources`, or from another repository through a `pin`.

```kdl
image {
    modules {
        module "core/bootloader"
    }
}
```

Accepts: {&nbsp;[fields](#image-modules-fields)&nbsp;}

<a id="image-modules-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`module`](#image-modules-module) (required,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#image-modules-module-fields)&nbsp;} | One module of the image, which the list entry names by its path under `modules/`. |
| [`source`](#image-modules-source) (optional,&nbsp;repeatable) | *string*, then {&nbsp;[fields](#image-modules-source-fields)&nbsp;} | The modules that the image references from one of the collections in `sources`. |
| [`flavour`](#image-modules-flavour) (optional,&nbsp;repeatable) | *string*, then {&nbsp;[fields](#image-modules-flavour-fields)&nbsp;} | The modules one flavour adds, which build only for that flavour. |

<a id="image-modules-module"></a>

#### `module` (required, repeatable)

Adds one module to the image. The entry can also set the options that the module declares, or select one of its variants.

```kdl
image {
    modules {
        module "core/bootloader"
    }
}
```

Accepts: *string*, then optionally {&nbsp;[fields](#image-modules-module-fields)&nbsp;}

A list entry with a `pin` names a module in another repository:

- `tect` fetches the module, verifies it against the hash and unpacks it under `modules/.remote/`.
- The URL of the pin is `https` or `file`, and it points at a tar archive. It expands `{version}` and no other placeholder.
- The fetched module ships a `module.kdl` that the same schema holds.
- `tect` fetches nothing that the fetched module requires. If the module needs another module, then the image lists that module too.

| Property | Accepts | Description |
| --- | --- | --- |
| `variant=` (optional) | *string* | Which of the module's declared variants this image builds. |

<a id="image-modules-module-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`pin`](#image-modules-module-pin) (optional) | {&nbsp;[fields](#image-modules-module-pin-fields)&nbsp;} | Where content comes from, which version it is, what verifies it, and what keeps the version current. |
| [`<name>`](#image-modules-module-name) (optional,&nbsp;repeatable) |  | An option that the module declares, which this image sets in a child node named after the option. |

> [!NOTE]
> A nested name such as `hardening/coredumps` names the directories that the collection groups its members in. Each part of the name is a name in its own right.

<a id="image-modules-module-pin"></a>

##### `pin` (optional)

A pin fixes where a download comes from and which version it is, and says how the version stays current. Unless the pin is `unpinned`, `tect` verifies each download against the pinned hash, so a changed upstream file is caught.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                renovate datasource="github-tags" depName="owner/repo"
                version "v1.0.0"
                url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
                sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
            }
        }
    }
}
```

Accepts: {&nbsp;[fields](#image-modules-module-pin-fields)&nbsp;}

A pin holds exactly one of these, so every pin says how it stays current:

- `renovate`, if Renovate keeps the version current;
- `manual`, if nothing tracks the pin;
- `unpinned`, if the pin follows a moving ref with no `sha256`.

<a id="image-modules-module-pin-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`renovate`](#image-modules-module-pin-renovate) (optional) |  | The Renovate custom manager that keeps `version` current. |
| [`manual`](#image-modules-module-pin-manual) (optional) | *string* | Why nothing tracks this pin. |
| [`unpinned`](#image-modules-module-pin-unpinned) (optional) | *string* | Why this pin follows a moving ref with no `sha256`. |
| [`version`](#image-modules-module-pin-version) (required) | *string* | The version, tag or commit that `url` expands and Renovate rewrites. |
| [`url`](#image-modules-module-pin-url) (required) | *string* | Where the content comes from. |
| [`sha256`](#image-modules-module-pin-sha256) (optional) | *string* | The hash the fetched content must match. |
| [`path`](#image-modules-module-pin-path) (optional) | *string* | The directory inside the archive that holds the content. |

> [!NOTE]
> An `asset`, an out-of-tree module and a collection each hold a `pin`.
>
> A base holds no `pin`. Its image reference holds the location and the version, and `signed` records whether the base publishes a cosign signature.
>
> Renovate matches `renovate` together with the line directly below it. Put `version` on that line.

<a id="image-modules-module-pin-renovate"></a>

###### `renovate` (optional)

The Renovate custom manager that keeps `version` current.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                renovate datasource="github-tags" depName="owner/repo"
            }
        }
    }
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `datasource=` (required) | `github-releases` or `github-tags` or `git-refs` | The Renovate datasource that tracks the pin. |
| `depName=` (required) | *string* | The name the datasource knows the dependency by. |
| `extractVersion=` (optional) | *string* | The pattern that extracts the version from a tag. |

<a id="image-modules-module-pin-manual"></a>

###### `manual` (optional)

Why nothing tracks this pin.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                manual "upstream publishes no releases"
            }
        }
    }
}
```

Accepts: *string*

<a id="image-modules-module-pin-unpinned"></a>

###### `unpinned` (optional)

Why this pin follows a moving ref with no `sha256`.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                unpinned "followed at its branch head"
            }
        }
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

<a id="image-modules-module-pin-version"></a>

###### `version` (required)

The version, tag or commit that `url` expands and Renovate rewrites.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                version "v1.0.0"
            }
        }
    }
}
```

Accepts: *string*

<a id="image-modules-module-pin-url"></a>

###### `url` (required)

Where the content comes from.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
            }
        }
    }
}
```

Accepts: *string*

<a id="image-modules-module-pin-sha256"></a>

###### `sha256` (optional)

The hash the fetched content must match.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
            }
        }
    }
}
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `from=` (optional) | `asset` or `sidecar` or `manual` | Where the hash is refreshed from. `asset`, the default, hashes the payload itself. `sidecar` reads the `<url>.sha256` file that upstream publishes. `manual` means nothing recomputes the hash. |

> [!NOTE]
> If a pin has no `sha256` and no `unpinned`, then `tect check` reports an error.

<a id="image-modules-module-pin-path"></a>

###### `path` (optional)

The directory inside the archive that holds the content.

```kdl
image {
    modules {
        module "core/bootloader" {
            pin {
                path "modules/example"
            }
        }
    }
}
```

Accepts: *string*

<a id="image-modules-module-name"></a>

##### `<name>` (optional, repeatable)

An option that the module declares, which this image sets in a child node named after the option.

```kdl
image {
    modules {
        module "core/bootloader" {
            fonts "JetBrainsMono" "FiraCode"
        }
    }
}
```

<a id="image-modules-source"></a>

#### `source` (optional, repeatable)

Groups the modules that come from one collection in `sources`. Each entry inside names a module by its path within that collection.

```kdl
image {
    modules {
        source "tectonic-os" {
            module "core/bootloader"
        }
    }
}
```

Accepts: *string*, then {&nbsp;[fields](#image-modules-source-fields)&nbsp;}

<a id="image-modules-source-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`module`](#image-modules-module) (required,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#image-modules-module-fields)&nbsp;} | One module of the image, which the list entry names by its path under `modules/`. |

<a id="image-modules-flavour"></a>

#### `flavour` (optional, repeatable)

Lists the modules that build only for one flavour. Every module outside a `flavour` block builds for every target.

```kdl
image {
    modules {
        flavour "dev" {
            module "core/bootloader"
        }
    }
}
```

Accepts: *string*, then {&nbsp;[fields](#image-modules-flavour-fields)&nbsp;}

<a id="image-modules-flavour-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`module`](#image-modules-module) (required,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#image-modules-module-fields)&nbsp;} | One module of the image, which the list entry names by its path under `modules/`. |
| [`source`](#image-modules-source) (optional,&nbsp;repeatable) | *string*, then {&nbsp;[fields](#image-modules-source-fields)&nbsp;} | The modules that the image references from one of the collections in `sources`. |
