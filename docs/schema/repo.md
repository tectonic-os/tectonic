<a id="repo.kdl"></a>

# `repo.kdl`

Location: The root of the repository.

`repo.kdl` sits at the root of the repository and holds what applies to every image in it: which `tect` release builds it, where its modules come from, which CI workflows run and which security rules every image follows. The images themselves live in image files beside it.

```kdl
schema-version 1
name "Workstation"
workflows at="12:30" {
    build
}
sources {
    modules "tectonic-modules" {
        url "https://github.com/tectonic-os/library"
        path "modules"
    }
    base-images "core" {
        url "https://github.com/tectonic-os/library"
        path "base-images/core"
    }
    capabilities "core" {
        url "https://github.com/tectonic-os/library"
        path "capabilities"
    }
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`schema-version`](#schema-version) (required) | *number* | The schema release that this repository is written against. |
| [`tect-version`](#tect-version) (optional) | *string* | The `tect` release that builds this repository. |
| [`name`](#name) (optional) | *string* | An optional human-readable label for the repository. |
| [`default-image`](#default-image) (optional) | *string* | The image that a command with no image, or a build with no target, uses. |
| [`pr-image`](#pr-image) (optional) | *string* | The image a pull request builds. |
| [`seed`](#seed) (optional) | *string* | The image that this repository publishes as a starting point for a new repository. |
| [`workflows`](#workflows) (optional) | {&nbsp;[fields](#workflows-fields)&nbsp;} | The CI workflows that `tect generate` writes into `.github/workflows/`. |
| [`sources`](#sources) (optional) | {&nbsp;[fields](#sources-fields)&nbsp;} | The module, base-image and capability libraries this repository reads. |
| [`manifest`](#manifest) (optional) | {&nbsp;[fields](#manifest-fields)&nbsp;} | Whether a build stamps the generated manifest onto the image as an OCI label. |
| [`audit`](#audit) (optional) | {&nbsp;[fields](#audit-fields)&nbsp;} | How the repository treats a provenance check that fails. |
| [`security-policy`](#security-policy) (optional) | optionally {&nbsp;[fields](#security-policy-fields)&nbsp;} | The security rules that the repository sets for every image it defines. |

<a id="schema-version"></a>

## `schema-version` (required)

The schema version tells `tect` which reader to use for the repository. A repository written against an earlier release keeps building, because the version picks the reader. `tect create repo` writes it, and `tect` never changes it.

```kdl
schema-version 1
```

Accepts: *number*

> [!NOTE]
> The version picks the reader, so a repository written against an earlier release keeps building.
>
> If `tect` does not read the version, then it says so once and reports no node it cannot place.
>
> `tect` does not move a repository to a newer version. The release that the repository pins reads it.

<a id="tect-version"></a>

## `tect-version` (optional)

Pins the `tect` release that builds the repository, so every build uses the same tool. `tect.sh` fetches the pinned release, so a build does not depend on the `tect` installed on the machine.

```kdl
tect-version "0.6.49"
```

Accepts: *string*

If a different release of `tect` reads the repository:

- `tect` says once that the pin names a different release.
- `schema-version` decides whether that release can read the repository.
- `tect verify` reports the difference as drift.
- `tect generate` resolves the drift.

| Property | Accepts | Description |
| --- | --- | --- |
| `sha256=` (optional) | *string* | The sha256 that `tect.sh` holds the release tarball to. |

> [!NOTE]
> `tect.sh` fetches this release, so the build does not use the `tect` installed on the machine.
>
> If `sha256=` is absent, then `tect.sh` checks the checksum published beside the tarball. That check proves only that the download is complete.
>
> If the repository pins no release, then `tect` says nothing about the release.

<a id="name"></a>

## `name` (optional)

A human-readable label for the repository. It labels the result tree printed by commands that add or change files, and does not affect generation or builds. When it is omitted, that tree uses the repository directory name.

```kdl
name "Workstation"
```

Accepts: *string*

<a id="default-image"></a>

## `default-image` (optional)

If the repository defines more than one image, then this names the image that a command uses when the user names none. If the repository defines one image, then that image is the default and this field is not needed.

```kdl
default-image "workstation"
```

Accepts: *string*

<a id="pr-image"></a>

## `pr-image` (optional)

The image a pull request builds.

```kdl
pr-image "workstation"
```

Accepts: *string*

<a id="seed"></a>

## `seed` (optional)

A seed lets another user start a new repository from this one. `tect generate` writes the base, the module list and the collections of the named image into the seed, so a new repository starts from the same image.

```kdl
seed "workstation" collection="owner"
```

Accepts: *string*

`tect generate` writes the seed to `generated/seed.kdl`. The seed carries:

- the base of the image;
- the module list of the image;
- the collections that those modules come from.

| Property | Accepts | Description |
| --- | --- | --- |
| `collection=` (required) | *string* | The collection that this repository publishes its own modules as. |

> [!NOTE]
> The seed carries no name, URL or owner of this repository.
>
> The seed names each module through the collection it is fetched from, so `collection=` names a collection in `sources`. A repository is seedable only if it publishes its own `modules/` as a collection.
>
> If the seed image lists a module that no collection can import, then `tect` reports it, because a new repository could not build that seed.

<a id="workflows"></a>

## `workflows` (optional)

Names the GitHub Actions workflows that `tect generate` writes and keeps current, and sets when scheduled builds, publishing and scans run. The repository gets only the workflows named here.

```kdl
workflows at="12:30" {
    build
}
```

Accepts: {&nbsp;[fields](#workflows-fields)&nbsp;}

| Property | Accepts | Description |
| --- | --- | --- |
| `at=` (optional) | *string* | The hour and minute in UTC that the daily build runs at. Every other scheduled workflow runs at its own offset from it. |
| `publish=` (optional) | `scheduled` | If `scheduled`, then images publish from the daily build alone, and image scans move off pushes too, because a scan reads a published image. If absent, then images also publish on pushes. |
| `scan=` (optional) | `scheduled` | If `scheduled`, then image scans run on the daily build alone. If absent, then scans run on pushes and on scheduled builds. |

<a id="workflows-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#workflows-name) (optional,&nbsp;repeatable) |  | One workflow, which the stem of its file names. |

> [!NOTE]
> `tect generate` writes only the workflows named here. After a tool upgrade, it rewrites each named workflow.
>
> `tect set workflows` edits this block.
>
> The `conforms` of each image decides whether SCAP content exists to scan.

<a id="workflows-name"></a>

### `<name>` (optional, repeatable)

One workflow, which the stem of its file names.

```kdl
workflows at="12:30" {
    build
}
```

<a id="sources"></a>

## `sources` (optional)

Lists typed libraries that this repository reads from a local directory or fetches from an HTTPS Git repository. A `modules` source feeds imports and copies. A `base-images` source is the base catalog. A `capabilities` source locates the paths that witness a capability. Every source declares exactly one of `dir` and `url`: `dir` names a directory on this machine, and `url` names an HTTPS Git repository.

```kdl
sources {
    modules "tectonic-modules"
}
```

Accepts: {&nbsp;[fields](#sources-fields)&nbsp;}

A module from a collection reaches the repository in one of two ways:

- `tect import module` references the module under the name of its collection.
- `tect copy module` puts the module unqualified at `modules/<name>`, and `provenance.kdl` records the collection.

<a id="sources-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`modules`](#sources-modules) (optional,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#sources-modules-fields)&nbsp;} | A module library that imports and copies resolve against. |
| [`base-images`](#sources-base-images) (optional,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#sources-base-images-fields)&nbsp;} | A base-image library that the base catalog is read out of. |
| [`capabilities`](#sources-capabilities) (optional,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#sources-capabilities-fields)&nbsp;} | A capability library that locates capability witness paths. |

<a id="sources-modules"></a>

### `modules` (optional, repeatable)

A module library that imports and copies resolve against.

```kdl
sources {
    modules "tectonic-modules"
}
```

Accepts: *string*, then optionally {&nbsp;[fields](#sources-modules-fields)&nbsp;}

<a id="sources-modules-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`dir`](#sources-modules-dir) (optional) | *string* | The local directory that holds this source. |
| [`url`](#sources-modules-url) (optional) | *string* | The HTTPS Git repository that holds this source. |
| [`path`](#sources-modules-path) (optional) | *string* | The directory inside the Git repository that holds this module library. |
| [`version`](#sources-modules-version) (optional) | *string* | The branch, tag or commit that selects the repository tree. |
| [`sha256`](#sources-modules-sha256) (optional) | *string* | The hash of the canonical tar archive for the selected commit. |
| [`unpinned`](#sources-modules-unpinned) (optional) | *string* | Why this source follows a ref without a verified archive hash. |

<a id="sources-modules-dir"></a>

#### `dir` (optional)

The local directory that holds this source.

```kdl
sources {
    modules "tectonic-modules" {
        dir "../library/modules"
    }
}
```

Accepts: *string*

<a id="sources-modules-url"></a>

#### `url` (optional)

The HTTPS Git repository that holds this source.

```kdl
sources {
    modules "tectonic-modules" {
        url "https://github.com/tectonic-os/library"
    }
}
```

Accepts: *string*

<a id="sources-modules-path"></a>

#### `path` (optional)

The directory inside the Git repository that holds this module library.

```kdl
sources {
    modules "tectonic-modules" {
        path "modules"
    }
}
```

Accepts: *string*

<a id="sources-modules-version"></a>

#### `version` (optional)

The branch, tag or commit that selects the repository tree.

```kdl
sources {
    modules "tectonic-modules" {
        version "v1"
    }
}
```

Accepts: *string*

<a id="sources-modules-sha256"></a>

#### `sha256` (optional)

The hash of the canonical tar archive for the selected commit.

```kdl
sources {
    modules "tectonic-modules" {
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    }
}
```

Accepts: *string*

<a id="sources-modules-unpinned"></a>

#### `unpinned` (optional)

Why this source follows a ref without a verified archive hash.

```kdl
sources {
    modules "tectonic-modules" {
        unpinned "This repository follows the library's default ref"
    }
}
```

Accepts: *string*

<a id="sources-base-images"></a>

### `base-images` (optional, repeatable)

A base-image library that the base catalog is read out of.

```kdl
sources {
    base-images "core"
}
```

Accepts: *string*, then optionally {&nbsp;[fields](#sources-base-images-fields)&nbsp;}

<a id="sources-base-images-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`dir`](#sources-modules-dir) (optional) | *string* | The local directory that holds this source. |
| [`url`](#sources-modules-url) (optional) | *string* | The HTTPS Git repository that holds this source. |
| [`path`](#sources-base-images-path) (optional) | *string* | The directory inside the Git repository that holds this base-image library. |
| [`version`](#sources-modules-version) (optional) | *string* | The branch, tag or commit that selects the repository tree. |
| [`sha256`](#sources-modules-sha256) (optional) | *string* | The hash of the canonical tar archive for the selected commit. |
| [`unpinned`](#sources-modules-unpinned) (optional) | *string* | Why this source follows a ref without a verified archive hash. |

<a id="sources-base-images-path"></a>

#### `path` (optional)

The directory inside the Git repository that holds this base-image library.

```kdl
sources {
    base-images "core" {
        path "base-images/core"
    }
}
```

Accepts: *string*

<a id="sources-capabilities"></a>

### `capabilities` (optional, repeatable)

A capability library that locates capability witness paths.

```kdl
sources {
    capabilities "core"
}
```

Accepts: *string*, then optionally {&nbsp;[fields](#sources-capabilities-fields)&nbsp;}

<a id="sources-capabilities-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`dir`](#sources-modules-dir) (optional) | *string* | The local directory that holds this source. |
| [`url`](#sources-modules-url) (optional) | *string* | The HTTPS Git repository that holds this source. |
| [`path`](#sources-capabilities-path) (optional) | *string* | The directory inside the Git repository that holds this capability library. |
| [`version`](#sources-modules-version) (optional) | *string* | The branch, tag or commit that selects the repository tree. |
| [`sha256`](#sources-modules-sha256) (optional) | *string* | The hash of the canonical tar archive for the selected commit. |
| [`unpinned`](#sources-modules-unpinned) (optional) | *string* | Why this source follows a ref without a verified archive hash. |

<a id="sources-capabilities-path"></a>

#### `path` (optional)

The directory inside the Git repository that holds this capability library.

```kdl
sources {
    capabilities "core" {
        path "capabilities"
    }
}
```

Accepts: *string*

<a id="manifest"></a>

## `manifest` (optional)

The build writes a manifest of what went into the image into the image itself. This block decides whether the build also stamps an OCI label that points at that file, so a tool that reads image labels can find it.

```kdl
manifest {
    label #true
}
```

Accepts: {&nbsp;[fields](#manifest-fields)&nbsp;}

<a id="manifest-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`label`](#manifest-label) (optional) | *boolean* | Whether the build stamps `org.tectonic.manifest` with the path of the baked manifest file. |

<a id="manifest-label"></a>

### `label` (optional)

Whether the build stamps `org.tectonic.manifest` with the path of the baked manifest file.

```kdl
manifest {
    label #true
}
```

Accepts: *boolean*

<a id="audit"></a>

## `audit` (optional)

Every build records where its parts came from. This block decides whether a failed provenance check is only reported or stops the command, so a repository can move from reporting to enforcing when it is ready.

```kdl
audit {
    enforce #true
}
```

Accepts: {&nbsp;[fields](#audit-fields)&nbsp;}

`tect` records these provenance facts whether or not this block exists:

- the hash of each module;
- the source of each import;
- the digest of the base tag;
- the commit of each cloned asset.

<a id="audit-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`enforce`](#audit-enforce) (optional) | *boolean* | Whether a failed provenance check stops the command that runs it. |

<a id="audit-enforce"></a>

### `enforce` (optional)

Whether a failed provenance check stops the command that runs it.

```kdl
audit {
    enforce #true
}
```

Accepts: *boolean*

Each check runs in the command named before it:

- `tect import module` and `tect copy module`: the collection is pinned to a tag and its hash.
- `tect check`: each imported module still matches its import record.
- `tect build`: the base tag resolves to a manifest digest.
- `tect build`: the repository is at a commit.
- `tect scap`: the scan report passes every rule of the profile.

| Value | Effect |
| --- | --- |
| `#true` | A failed check is an error, and the command stops. |
| `#false`, or no `enforce` | `tect` reports a failed check, and the command continues. |

> [!NOTE]
> `tect` also reports a `module.sh` or `finalize.sh` that reaches the network with no `asset` that declares the download. This report does not depend on `enforce`.

<a id="security-policy"></a>

## `security-policy` (optional)

Sets the security rules that every image in the repository follows, whatever each image declares. It holds one rule: what each build step can reach on the network.

```kdl
security-policy
```

Accepts: optionally {&nbsp;[fields](#security-policy-fields)&nbsp;}

<a id="security-policy-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`network`](#security-policy-network) (optional) | optionally `allow` or `strict`, then optionally {&nbsp;[fields](#security-policy-network-fields)&nbsp;} | The rule that decides whether each step of a module layer reaches the network. |

> [!NOTE]
> If the `security-policy` block is absent, then every step keeps the network.

<a id="security-policy-network"></a>

### `network` (optional)

Controls network access while the image builds. The user can open the network to every step, or with `strict` open it only for the modules that declare a need, so an undeclared download fails the build. `packages` and `scripts` set the rule for one kind of step.

```kdl
security-policy {
    network "strict"
}
```

Accepts: optionally `allow` or `strict`, then optionally {&nbsp;[fields](#security-policy-network-fields)&nbsp;}

A module layer runs two kinds of step:

- A package step installs the repo files, the COPRs and the packages of the module.
- A script step runs the `module.sh` of the module and its finalize hook.

| Value | Effect |
| --- | --- |
| `allow` | Both kinds of step take `allow`, unless `packages` or `scripts` sets another rule. |
| `strict` | Both kinds of step take `strict`, unless `packages` or `scripts` sets another rule. |
| No value | Both kinds of step take `allow`, unless `packages` or `scripts` sets another rule. |

<a id="security-policy-network-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`packages`](#security-policy-network-packages) (optional) | `allow` or `strict` or `deny` | The rule that every package step runs under. |
| [`scripts`](#security-policy-network-scripts) (optional) | `allow` or `strict` or `deny` | The rule that every script step runs under. |

<a id="security-policy-network-packages"></a>

#### `packages` (optional)

The rule that every package step runs under.

```kdl
security-policy {
    network "strict" {
        packages "allow"
    }
}
```

Accepts: `allow` or `strict` or `deny`

| Value | Effect |
| --- | --- |
| `allow` | Every package step reaches the network. |
| `strict` | Every package step reaches the network, because a package step runs only where the module declares packages, a COPR or a repo file. |
| `deny` | Every package step runs under `--network=none`. |

<a id="security-policy-network-scripts"></a>

#### `scripts` (optional)

The rule that every script step runs under.

```kdl
security-policy {
    network "strict" {
        scripts "strict"
    }
}
```

Accepts: `allow` or `strict` or `deny`

| Value | Effect |
| --- | --- |
| `allow` | Every script step reaches the network. |
| `strict` | A script step reaches the network only if its module declares `network "scripts"`. |
| `deny` | Every script step runs under `--network=none`. |

> [!NOTE]
> If the rule is `strict` or `deny`, then `tect check` names each module that ships a Containerfile fragment. The rule does not reach the `RUN` lines of a fragment.
