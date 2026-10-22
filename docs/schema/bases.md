<a id="<name>.base.kdl"></a>

# `<name>.base.kdl`

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

## `base` (required)

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

### `image` (required)

The image reference that an image on this base builds from.

```kdl
base {
    image "quay.io/fedora/fedora-bootc:44"
}
```

Accepts: *string*

<a id="base-about"></a>

### `about` (required)

The line that the base picker shows beside the reference.

```kdl
base {
    about "Fedora bootc, the upstream base image"
}
```

Accepts: *string*

<a id="base-family"></a>

### `family` (required)

The family that an image on this base declares, which each module's `supports` must match.

```kdl
base {
    family "fedora"
}
```

Accepts: *string*

<a id="base-provides"></a>

### `provides` (optional, repeatable)

The capabilities that this base already ships.

```kdl
base {
    provides "rechunking" "bootc"
}
```

Accepts: *list of strings*

<a id="base-requires"></a>

### `requires` (optional, repeatable)

The capabilities that an enabled module must provide before this base is usable.

```kdl
base {
    requires "bootc-base"
}
```

Accepts: *list of strings*

<a id="base-signed"></a>

### `signed` (optional)

Whether this base publishes a cosign signature.

```kdl
base {
    signed #true
}
```

Accepts: *boolean*

<a id="base-scap-content"></a>

### `scap-content` (optional)

The bare filename of the SSG datastream that measures this base.

```kdl
base {
    scap-content "ssg-fedora-ds.xml"
}
```

Accepts: *string*

<a id="base-bootloader"></a>

### `bootloader` (required)

The bootloaders that an image on this base can install, with the default first.

```kdl
base {
    bootloader "grub2"
}
```

Accepts: *list of strings*
