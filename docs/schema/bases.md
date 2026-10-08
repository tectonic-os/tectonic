# Base images and capabilities

<a id="<name>.base.kdl"></a>

## `<name>.base.kdl`

Location: `base-images/<library>/<name>.base.kdl`, in a `base-images` source.

One file describes one base, and its file stem is the base's catalog name. A `base-images` source reads every `*.base.kdl` file in the directory its `path` names.

```kdl
base {
    image "quay.io/fedora/fedora-bootc:44"
    about "Fedora bootc, the upstream base image"
    family "fedora"
    bootloader "grub2"
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`base`](#base) (required) | {&nbsp;[fields](#base-fields)&nbsp;} | One base that an image builds on, which the file stem names. |

> [!NOTE]
> The file stem is the base's catalog name: `fedora-bootc-44.base.kdl` describes the base `fedora-bootc-44`.
>
> Two files that name one image reference are refused, because which of them an image got its family and its `provides` from would depend on the order the sources are read in.

<a id="base"></a>

### `base` (required)

One file describes one base: what it is, which family it belongs to, what it ships and what it still needs. `tect create image` copies these facts into each image that it scaffolds on the base.

```kdl
base {
    image "quay.io/fedora/fedora-bootc:44"
    about "Fedora bootc, the upstream base image"
    family "fedora"
    bootloader "grub2"
}
```

Accepts: {&nbsp;[fields](#base-fields)&nbsp;}

<a id="base-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`image`](#base-image) (required) | *string* | The image reference that an image on this base builds from. |
| [`about`](#base-about) (required) | *string* | The line that the base picker shows beside the reference. |
| [`family`](#base-family) (required) | *string* | The family that an image on this base declares, which each module's `supports` must match. |
| [`provides`](#base-provides) (optional,&nbsp;repeatable) | *list of strings* | The capabilities that this base already ships. |
| [`requires`](#base-requires) (optional,&nbsp;repeatable) | *list of strings* | The capabilities that an enabled module must provide before this base is usable. |
| [`signed`](#base-signed) (optional) | *boolean* | Whether this base publishes a cosign signature. |
| [`scap-content`](#base-scap-content) (optional) | *string* | The bare filename of the SSG datastream that measures this base. |
| [`bootloader`](#base-bootloader) (required) | *list of strings* | The bootloaders that an image on this base can install, with the default first. |

> [!NOTE]
> `tect create image` writes `family`, `provides`, `requires`, `signed` and `bootloader` into each image it scaffolds on this base.
>
> No catalog row carries a digest, because a digest in the catalog changes only with a library commit. The image file that builds on the base holds the digest.

<a id="base-image"></a>

#### `image` (required)

The image reference that an image on this base builds from.

```kdl
base {
    image "quay.io/fedora/fedora-bootc:44"
}
```

Accepts: *string*

<a id="base-about"></a>

#### `about` (required)

The line that the base picker shows beside the reference.

```kdl
base {
    about "Fedora bootc, the upstream base image"
}
```

Accepts: *string*

<a id="base-family"></a>

#### `family` (required)

The family that an image on this base declares, which each module's `supports` must match.

```kdl
base {
    family "fedora"
}
```

Accepts: *string*

<a id="base-provides"></a>

#### `provides` (optional, repeatable)

The capabilities that this base already ships.

```kdl
base {
    provides "rechunking" "bootc"
}
```

Accepts: *list of strings*

<a id="base-requires"></a>

#### `requires` (optional, repeatable)

The capabilities that an enabled module must provide before this base is usable.

```kdl
base {
    requires "bootc-base"
}
```

Accepts: *list of strings*

<a id="base-signed"></a>

#### `signed` (optional)

Whether this base publishes a cosign signature.

```kdl
base {
    signed #true
}
```

Accepts: *boolean*

<a id="base-scap-content"></a>

#### `scap-content` (optional)

The bare filename of the SSG datastream that measures this base.

```kdl
base {
    scap-content "ssg-fedora-ds.xml"
}
```

Accepts: *string*

<a id="base-bootloader"></a>

#### `bootloader` (required)

The bootloaders that an image on this base can install, with the default first.

```kdl
base {
    bootloader "grub2"
}
```

Accepts: *list of strings*

<a id="capabilities.kdl"></a>

## `capabilities.kdl`

Location: `capabilities.kdl`, at the root of a `capabilities` source.

One file locates the witnesses a base and a module name. A capabilities source reads this one file at the root of the directory its `path` names.

```kdl
capability "ssh"
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`capability`](#capability) (optional,&nbsp;unique) | *string*, then optionally {&nbsp;[fields](#capability-fields)&nbsp;} | Where the presence of a capability is read, which is the path that witnesses it. |

> [!NOTE]
> A capability that no row names is read at `/usr/bin/<name>` then `/usr/sbin/<name>`, where a probe measures a base.
>
> A row with no path names an abstract capability. It suppresses the conventional lookup, so the name is never witnessed and no build stops on it.

<a id="capability"></a>

### `capability` (optional, unique)

Tells `tect` how to prove that a capability is in a finished image: the path of a file that witnesses it. A bare `path` holds for every family, and a `family` block reads a different path on the families it names. A row is needed only where the witness is not `/usr/bin/<name>` or `/usr/sbin/<name>`.

```kdl
capability "ssh"
```

Accepts: *string*, then optionally {&nbsp;[fields](#capability-fields)&nbsp;}

A path witnesses a capability name. `tect` reads the first of these that names one:

- the `file=` of a module's `provides "<name>"`;
- the capability row for the image's family;
- `/usr/bin/<name>` or `/usr/sbin/<name>`, where the base probe measures a base and no row names the capability at all.

<a id="capability-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`path`](#capability-path) (optional) | *string* | The absolute path whose presence witnesses the capability. |
| [`family`](#capability-family) (optional,&nbsp;repeatable) | *list of strings*, then {&nbsp;[fields](#capability-family-fields)&nbsp;} | A path that overrides the capability's default on the families it names. |

> [!NOTE]
> A row with no path names a capability that nothing witnesses. It also suppresses the probe's conventional lookup, so a row is how a name with no file anywhere stays abstract on purpose.
>
> `tect validate-image` checks the finished image for each name that the base claims, and for each name that a module or a row locates. `base-sig-probe` reads the same witnesses when it measures a base.
>
> The witness of `luks-initramfs` is inside an archive. If a target declares it, then the build validates the initramfs with `lsinitrd`. If no target declares it, then the installer does not offer root encryption.
>
> Declare `luks-initramfs` in a custom base or module only if its initramfs unlocks LUKS. `tect build` then proves that the binary is present before the installer offers it.

<a id="capability-path"></a>

#### `path` (optional)

The absolute path whose presence witnesses the capability.

```kdl
capability "ssh" {
    path "/usr/sbin/sshd"
}
```

Accepts: *string*

<a id="capability-family"></a>

#### `family` (optional, repeatable)

A path that overrides the capability's default on the families it names.

```kdl
capability "ssh" {
    family "debian" "ubuntu" {
        path "/usr/bin/ssh"
    }
}
```

Accepts: *list of strings*, then {&nbsp;[fields](#capability-family-fields)&nbsp;}

<a id="capability-family-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`path`](#capability-family-path) (required) | *string* | The absolute path that witnesses the capability on these families. |

<a id="capability-family-path"></a>

##### `path` (required)

The absolute path that witnesses the capability on these families.

```kdl
capability "ssh" {
    family "debian" "ubuntu" {
        path "/usr/bin/ssh"
    }
}
```

Accepts: *string*
