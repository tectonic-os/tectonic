<a id="bases.kdl"></a>

# `bases.kdl`

Location: Compiled into `tect`. A `bases.kdl` beside the `tect` binary replaces it, and one at the root of a collection extends it.

The base catalog lists the upstream images that `tect` knows, and what each one ships. `tect create image` offers these bases and writes what each ships into the new image.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    about "Fedora bootc, the upstream base image"
    family "fedora"
    bootloader "grub2"
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`base`](#base) (required,&nbsp;unique) | *string*, then {&nbsp;[fields](#base-fields)&nbsp;} | One base that an image builds on, which the catalog entry names by its image reference. |
| [`capability`](#capability) (optional,&nbsp;unique) | *list of strings* | Where the presence of a capability is read, which is the path that witnesses it. |

`tect` selects one catalog:

- Each `tect` release compiles one `bases.kdl` into the binary, and the release ships the same file for replacement at runtime.
- If a `bases.kdl` sits beside the binary, then it replaces the compiled-in catalog completely. The two do not merge.
- If the runtime `bases.kdl` is unreadable or malformed, then `tect` reports its exact path. It does not fall back to the compiled-in catalog.

A collection extends the selected catalog with a `bases.kdl` at its root, beside its modules:

- A collection entry replaces the catalog entry for the same reference. `tect check` names the collection wherever the two entries differ.
- If two collections describe one base, then `tect` reports an error.
- `tect` fetches nothing to read a catalog. A collection that is not on this machine extends nothing.

> [!NOTE]
> `tect create image` offers the bases it knows, with the family of each and what each ships. An image scaffolded on a base lists no module that the base already carries.

<a id="base"></a>

## `base` (required, unique)

One entry describes one upstream base image: what it is, which family it belongs to, what it ships and what it still needs. `tect create image` copies these facts into each image that it scaffolds on the base.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    about "Fedora bootc, the upstream base image"
    family "fedora"
    bootloader "grub2"
}
```

Accepts: *string*, then {&nbsp;[fields](#base-fields)&nbsp;}

<a id="base-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
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
> No catalog row carries a digest, because a digest in the catalog changes only with a tool release. The image file that builds on the base holds the digest.

<a id="base-about"></a>

### `about` (required)

The line that the base picker shows beside the reference.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    about "Fedora bootc, the upstream base image"
}
```

Accepts: *string*

<a id="base-family"></a>

### `family` (required)

The family that an image on this base declares, which each module's `supports` must match.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    family "fedora"
}
```

Accepts: *string*

<a id="base-provides"></a>

### `provides` (optional, repeatable)

The capabilities that this base already ships.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    provides "rechunking" "bootc"
}
```

Accepts: *list of strings*

<a id="base-requires"></a>

### `requires` (optional, repeatable)

The capabilities that an enabled module must provide before this base is usable.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    requires "bootc-base"
}
```

Accepts: *list of strings*

<a id="base-signed"></a>

### `signed` (optional)

Whether this base publishes a cosign signature.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    signed #true
}
```

Accepts: *boolean*

<a id="base-scap-content"></a>

### `scap-content` (optional)

The bare filename of the SSG datastream that measures this base.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    scap-content "ssg-fedora-ds.xml"
}
```

Accepts: *string*

<a id="base-bootloader"></a>

### `bootloader` (required)

The bootloaders that an image on this base can install, with the default first.

```kdl
base "quay.io/fedora/fedora-bootc:44" {
    bootloader "grub2"
}
```

Accepts: *list of strings*

<a id="capability"></a>

## `capability` (optional, unique)

Tells `tect` how to prove that a capability is in a finished image: the path of a file that witnesses it. A row is needed only where the witness is not `/usr/bin/<name>` or `/usr/sbin/<name>`.

```kdl
capability "ssh" "/usr/sbin/sshd"
```

Accepts: *list of strings*

A path witnesses a capability name. `tect` reads the first of these that names one:

- the `file=` of a module's `provides "<name>"`;
- a `capability` row;
- `/usr/bin/<name>` or `/usr/sbin/<name>`.

> [!NOTE]
> A row with no path names a capability that nothing witnesses.
>
> `tect validate-image` checks the finished image for each name that the base claims, and for each name that a module or a row locates. `base-sig-probe` reads the same witnesses when it measures a base.
>
> The witness of `luks-initramfs` is inside an archive. If a target declares it, then the build validates the initramfs with `lsinitrd`. If no target declares it, then the installer does not offer root encryption.
>
> Declare `luks-initramfs` in a custom base or module only if its initramfs unlocks LUKS. `tect build` then proves that the binary is present before the installer offers it.
