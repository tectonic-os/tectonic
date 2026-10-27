<a id="image.kdl"></a>

# `image.kdl`

Location: The root of the repository, as `image.kdl` or `<name>.image.kdl`.

An image file holds its schema version and one or more images. Each image says what it is called, which base it builds on and which components it is made of.

```kdl
schema-version 1
image {
    name "Workstation"
    url "https://github.com/owner/workstation"
    issues-url "https://github.com/owner/workstation/issues"
    layout {
        bootloader "grub2"
    }
    desktop-environment
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`schema-version`](#schema-version) (required) | *number* | The schema release that this file is written against. |
| [`image`](#image) (required,&nbsp;repeatable) | {&nbsp;[fields](#image-fields)&nbsp;} | One image, which declares its names and every component that it is made of, its base among them. |

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
> `tect create repo`, `tect create image` and `tect create component` write the version, and `tect` never changes it.

<a id="image"></a>

## `image` (required, repeatable)

Declares one bootable operating system image: its names, how the installer lays it down and the components that make it up. Each component is a path of segments, such as `desktop-environment { kde-plasma }`, and one of them is the base that every layer builds on. The build publishes the image under its `id`.

```kdl
image {
    name "Workstation"
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
| [`boot-chain`](#image-boot-chain) (optional) | `uki-shim` or `uki-db` | The UKI boot chain, which is owner-signed through shim or enrolled directly into Secure Boot. |
| [`layout`](#image-layout) (optional) | {&nbsp;[fields](#image-layout-fields)&nbsp;} | What the installer lays down if the answer of the base family is not the one that the image wants. |
| [`allow-remediation`](#image-allow-remediation) (optional,&nbsp;unique) | *string* | One rule an installed component refuses that this image lets remediation set anyway. |
| [`flavours`](#image-flavours) (optional) | {&nbsp;[fields](#image-flavours-fields)&nbsp;} | The flavours that this image publishes beside its ungated build. |
| [`flavour`](#image-flavour) (optional,&nbsp;repeatable) | *string* | The components one flavour adds, which build only for that flavour. |
| [`<name>`](#image-name) (optional,&nbsp;repeatable) |  | One part of a component path, which lists the component at that path or holds further parts. |

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

<a id="image-boot-chain"></a>

### `boot-chain` (optional)

Builds the image to boot as a unified kernel image (UKI) under Secure Boot. `uki-shim` boots through shim with a key the owner signs, and `uki-db` enrolls the key directly into the firmware key database.

```kdl
image {
    boot-chain "uki-shim"
}
```

Accepts: `uki-shim` or `uki-db`

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

A component can refuse a hardening rule because the rule breaks what the component does. This lets one image accept that rule anyway, for a machine where the reason of the component does not apply. `because=` records why.

```kdl
image {
    allow-remediation "grub2_nousb_argument" because="this machine has no USB keyboard"
}
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `because=` (optional) | *string* | Why this image overrides the component's judgement. |

<a id="image-flavours"></a>

### `flavours` (optional)

A flavour builds a variant of the same image with extra components, such as a `dev` build with developer tools, and publishes it as `<id>-<flavour>`. Flavours suit two builds that share almost every component.

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
| [`<name>`](#image-flavours-name) (optional,&nbsp;repeatable) |  | One flavour that publishes a gated component set as `<id>-<flavour>`, where `<flavour>` is the node name. |

> [!NOTE]
> An image builds one target for its ungated component set and one more for each flavour. The targets publish as `<id>` and `<id>-<flavour>`.
>
> A flavour gates components inside one image. A component listed inside `flavour "dev"` builds only for that target, and each other component builds for every target.
>
> The `conforms=` of a flavour measures that flavour. The `conforms` of the image measures the ungated target only.

<a id="image-flavours-name"></a>

#### `<name>` (optional, repeatable)

One flavour that publishes a gated component set as `<id>-<flavour>`, where `<flavour>` is the node name.

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

<a id="image-flavour"></a>

### `flavour` (optional, repeatable)

Holds the path segments of the components that build only for one flavour. Every component outside a `flavour` block builds for every target.

```kdl
image {
    flavour "dev"
}
```

Accepts: *string*

<a id="image-name"></a>

### `<name>` (optional, repeatable)

Names one directory of a component path under `components/`. A segment lists the component at its path. If no component is there, then the segment only holds the segments below it. A child with an argument sets an option that the component declares, such as `version "44"`. A child with no argument is the next segment. A segment that holds a manifest node, such as `packages` or `base`, defines its component inline, and no directory is needed.

```kdl
image {
    desktop-environment
}
```

> [!NOTE]
> A component that the image file defines inline must not also be a component under `components/`. `tect check` refuses the pair and names both.
>
> `tect import component` copies a component from a library into `components/` and records where it came from in `components/components.lock.kdl`.
>
> The base is the listed component that declares `base`. Its segment also takes `digest` and `signed`, which record the digest and the signature state of its reference.
