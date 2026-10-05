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
    tectonic-os {
        pin {
            unpinned "followed at its branch head"
            version "v1.0.0"
            url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
        }
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
| [`sources`](#sources) (optional) | {&nbsp;[fields](#sources-fields)&nbsp;} | The module collections that `tect import module` and `tect copy module` resolve against. |
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

Lists the module collections that this repository takes modules from. A collection is a shared set of modules, which `tect` fetches as a pinned archive or reads from a directory on this machine. `tect import module` and `tect copy module` look modules up here.

```kdl
sources {
    tectonic-os
}
```

Accepts: {&nbsp;[fields](#sources-fields)&nbsp;}

A module from a collection reaches the repository in one of two ways:

- `tect import module` references the module under the name of its collection.
- `tect copy module` puts the module unqualified at `modules/<name>`, and `provenance.kdl` records the collection.

<a id="sources-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`<name>`](#sources-name) (optional,&nbsp;repeatable) | *string*, then optionally {&nbsp;[fields](#sources-name-fields)&nbsp;} | One module collection, where the node name is the owner that its references use. |

<a id="sources-name"></a>

### `<name>` (optional, repeatable)

A collection is a set of modules that another repository publishes. The node name is the owner name that module references use. A `pin` or a location says where the collection comes from.

```kdl
sources {
    tectonic-os
}
```

Accepts: *string*, then optionally {&nbsp;[fields](#sources-name-fields)&nbsp;}

A collection is one of two kinds:

- An archive, if the collection holds a `pin`. `tect` fetches and verifies it like an out-of-tree module.
- A directory on this machine, if the collection has a location argument. The location is relative to the repository root. `tect` reads it in place, and nothing is downloaded, pinned or hashed. The user can change the collection with no new archive for each edit.

If the pin of a collection carries `unpinned`, then the collection follows a branch:

- `tect create repo` scaffolds the collection that way.
- Every import or copy downloads the branch again, and nothing checks what arrived.
- Under `audit { enforce #true }`, an import or a copy is an error.
- Without enforcement, an import runs the unverified content, and a copy lands in the tracked tree for review.
- `tect check` names the collection above its counts and does not treat it as an error.

<a id="sources-name-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`pin`](#sources-name-pin) (optional) | {&nbsp;[fields](#sources-name-pin-fields)&nbsp;} | Where content comes from, which version it is, what verifies it, and what keeps the version current. |

> [!NOTE]
> `tect` checks a collection location when a command reads the collection. A check of the repository does not check it, because a directory can exist on one machine only.
>
> A tagged collection verifies every fetch, and every module in it shares the one version.

<a id="sources-name-pin"></a>

#### `pin` (optional)

A pin fixes where a download comes from and which version it is, and says how the version stays current. Unless the pin is `unpinned`, `tect` verifies each download against the pinned hash, so a changed upstream file is caught.

```kdl
sources {
    tectonic-os {
        pin {
            renovate datasource="github-tags" depName="owner/repo"
            version "v1.0.0"
            url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
            sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
        }
    }
}
```

Accepts: {&nbsp;[fields](#sources-name-pin-fields)&nbsp;}

A pin holds exactly one of these, so every pin says how it stays current:

- `renovate`, if Renovate keeps the version current;
- `manual`, if nothing tracks the pin;
- `unpinned`, if the pin follows a moving ref with no `sha256`.

<a id="sources-name-pin-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`renovate`](#sources-name-pin-renovate) (optional) |  | The Renovate custom manager that keeps `version` current. |
| [`manual`](#sources-name-pin-manual) (optional) | *string* | Why nothing tracks this pin. |
| [`unpinned`](#sources-name-pin-unpinned) (optional) | *string* | Why this pin follows a moving ref with no `sha256`. |
| [`version`](#sources-name-pin-version) (required) | *string* | The version, tag or commit that `url` expands and Renovate rewrites. |
| [`url`](#sources-name-pin-url) (required) | *string* | Where the content comes from. |
| [`sha256`](#sources-name-pin-sha256) (optional) | *string* | The hash the fetched content must match. |
| [`path`](#sources-name-pin-path) (optional) | *string* | The directory inside the archive that holds the content. |

> [!NOTE]
> An `asset`, an out-of-tree module and a collection each hold a `pin`.
>
> A base holds no `pin`. Its image reference holds the location and the version, and `signed` records whether the base publishes a cosign signature.
>
> Renovate matches `renovate` together with the line directly below it. Put `version` on that line.

<a id="sources-name-pin-renovate"></a>

##### `renovate` (optional)

The Renovate custom manager that keeps `version` current.

```kdl
sources {
    tectonic-os {
        pin {
            renovate datasource="github-tags" depName="owner/repo"
        }
    }
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `datasource=` (required) | `github-releases` or `github-tags` or `git-refs` | The Renovate datasource that tracks the pin. |
| `depName=` (required) | *string* | The name the datasource knows the dependency by. |
| `extractVersion=` (optional) | *string* | The pattern that extracts the version from a tag. |

<a id="sources-name-pin-manual"></a>

##### `manual` (optional)

Why nothing tracks this pin.

```kdl
sources {
    tectonic-os {
        pin {
            manual "upstream publishes no releases"
        }
    }
}
```

Accepts: *string*

<a id="sources-name-pin-unpinned"></a>

##### `unpinned` (optional)

Why this pin follows a moving ref with no `sha256`.

```kdl
sources {
    tectonic-os {
        pin {
            unpinned "followed at its branch head"
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

<a id="sources-name-pin-version"></a>

##### `version` (required)

The version, tag or commit that `url` expands and Renovate rewrites.

```kdl
sources {
    tectonic-os {
        pin {
            version "v1.0.0"
        }
    }
}
```

Accepts: *string*

<a id="sources-name-pin-url"></a>

##### `url` (required)

Where the content comes from.

```kdl
sources {
    tectonic-os {
        pin {
            url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
        }
    }
}
```

Accepts: *string*

<a id="sources-name-pin-sha256"></a>

##### `sha256` (optional)

The hash the fetched content must match.

```kdl
sources {
    tectonic-os {
        pin {
            sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
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

<a id="sources-name-pin-path"></a>

##### `path` (optional)

The directory inside the archive that holds the content.

```kdl
sources {
    tectonic-os {
        pin {
            path "modules/example"
        }
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
