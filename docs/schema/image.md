# Image files

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
        provides "rechunking" "initramfs-generation" "selinux-policy" "bootc"
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

`base { provides }` is what the upstream image already ships, by name. A
listed module whose every `provides` the base already carries is
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
| `conforms` | a string, at most one | The benchmark profile a scan measures the ungated target against. A scan reports it and enforces nothing. |
| `boot` | `uki-shim`, `uki-db`, at most one | The UKI boot chain: owner-signed through shim, or through direct Secure Boot enrollment. |

#### `base`

The image every layer builds on, and what building on it may assume.

*a string, exactly one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `family` | a string, exactly one | The base's family, matched against every module's `supports`. |
| `provides` | one or more strings | Capabilities the upstream image already ships, by name; a module providing only these is suppressed, and the finished image is checked for each. |
| `requires` | one or more strings | Capabilities the base is unusable without, which an enabled module must provide. |
| `signed` | `#true` or `#false`, at most one | Whether the base publishes a cosign signature. |

##### `satisfies`

Benchmarks and rules the base image already satisfies, as an audit declaration. The tool records it and certifies nothing.

*at most one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `<name>` | one or more strings, one per name | One benchmark, and the rule IDs it covers. |

#### `layout`

What the installer lays down, where the family's answer is not the one the image wants.

*at most one, never empty*

| Node | Takes | Meaning |
| --- | --- | --- |
| `composefs` | `#true` or `#false`, at most one | Whether the install seals the deployment, in place of the family's answer. |
| `generic-image` | `#true` or `#false`, at most one | Whether the install skips the bootupd check, in place of the family's answer. |
| `admin-group` | a string, at most one | The group an administrator is created in, in place of the family's. |
| `bootloader` | `grub2`, `systemd`, at most one | The bootloader the installer installs, one the base's row lists. |

##### `filesystem`

The root filesystem, in place of the one the base family settles.

*`xfs`, `ext4`, `btrfs`, `zfs`, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `subvolumes=` | `#true` or `#false` | btrfs only: create `@`, `@home` and `@snapshots`. |
| `pool=` | a string | zfs only: the pool to create. Fisherman's own default is `rpool`. |

##### `var-disk`

A whole disk the installer mounts at `/var`, named by its device.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `keep-existing=` | `#true` or `#false` | Whether the disk is mounted as it is; off formats it. |

#### `allow-remediation`

One rule an installed module refuses that this image lets remediation set anyway.

*a string, one per name*

| Property | Value | Meaning |
| --- | --- | --- |
| `because=` | a string | Why this image overrides the module's judgement. |

#### `flavours`

The flavours this image publishes beside its ungated build.

*at most one, never empty*

##### `<name>`

One flavour, named by the node: a gated module set published as `<image>-<flavour>`.

| Property | Value | Meaning |
| --- | --- | --- |
| `default=` | `#true` or `#false` | Whether a build that names no flavour builds this one. |
| `pr-build=` | `#true` or `#false` | Whether a pull request builds this flavour in place of the default. |
| `conforms=` | a string | The benchmark profile a scan measures this flavour against; the image's `conforms` measures the ungated target alone. |

#### `modules`

Every module the image is made of: ungated entries, and the flavours that gate the rest.

*exactly one*

##### `module`

One module the image is made of, named by its path under `modules/`.

*a string*

| Property | Value | Meaning |
| --- | --- | --- |
| `variant=` | a string | Which of the module's declared variants this image builds. |

Also holds [`pin`](pins.md#pin).

| Node | Takes | Meaning |
| --- | --- | --- |
| `<name>` |  | An option the module declares, set for this image by the node's name. |

##### `source`

Modules referenced from one of the collections in `sources`.

*a string*

Also holds `module`, as above.

##### `flavour`

The modules one flavour adds, which build only for that flavour.

*a string*

Also holds `module`, as above and `source`, as above.

<!-- /schema: image -->

## Out-of-tree modules

A list entry may name a module that lives in another repository. It is
fetched, verified against the hash, and unpacked under `modules/.remote/`. A
name that nests, `hardening/coredumps`, names the directories the collection
groups its members in; every part of it is a name in its own right.

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
