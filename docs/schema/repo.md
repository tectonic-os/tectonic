# The repository file

`repo.kdl` holds what is true of the repository rather than of any image in
it.

```kdl
schema-version 1

// renovate: datasource=github-releases depName=tectonic-os/tectonic
tect-version "0.0.0"

default-image "workstation"
pr-image "workstation"

workflows at="12:30" scan="scheduled" {
    build
    smoke-test
}
```

`schema-version` picks the reader, so a repository written against an earlier
release keeps building: a tool that does not know the version says so plainly
instead of reporting every node it cannot place. A repository at a version
this release does not read is read by nothing here, and nothing here moves it
forward; the release it pins is what reads it.

`tect-version` is that release. `scripts/tect.sh` fetches it, so the build runs
the tool the repository was written for whatever is installed on the machine.
Another release still reads the repository — `schema-version` is what decides
that — and says once that the pin names a different one, because what differs is
the output it would generate rather than the declarations it can parse. `verify`
reports that as drift and `tect generate` resolves it. A repository that pins
nothing is silent.

A workflow is named by its file stem under `.github/workflows/`, and `tect
generate` writes exactly the ones named here — so one nobody names is absent
rather than present and switched off, and a tool upgrade re-syncs the rest by
regenerating them. `tect set workflows` is the editor for the block. `at` is
the one time the schedules hang off: the daily build runs then, and every other
scheduled workflow at its own offset from it.

`scan="scheduled"` keeps image scans on the daily build but omits them from
push builds. Without it, scans run on both pushes and scheduled builds. Whether
there is SCAP content to scan remains derived from each image's `conforms`.

`sources` is the registry `tect import module <name>` and `tect copy module
<name>` resolve against. Importing references a member under the collection's
name; copying vendors it unqualified at `modules/<name>` and records the
collection in `provenance.kdl`.

```kdl
sources {
    tectonic-os {
        pin {
            renovate datasource="github-tags" depName="tectonic-os/modules"
            version "v1.0.0"
            url "https://github.com/tectonic-os/modules/archive/refs/tags/{version}.tar.gz"
            sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
        }
    }
    scratch "../modules"
}
```

A collection carrying a `pin` is an archive, fetched and verified exactly like
an out-of-tree module. One carrying a location instead is a directory on this
machine, relative to the repository root, which is read where it is: nothing is
downloaded, so there is nothing to pin or hash. That is what makes iterating on
a collection possible without re-tarring it on every edit.

A collection location is checked when something reads it, not when the
repository is checked: a directory that exists on one machine and not another
is not a repository problem.

### Collections that are not pinned

A collection may instead follow a branch, and `unpinned` is what says so. That
is what `create repo` scaffolds, and it buys convenience with verification:

```kdl
sources {
    tectonic-os {
        pin {
            unpinned "the collection this tool is published alongside, followed at its branch head"
            version "main"
            url "https://github.com/tectonic-os/modules/archive/refs/heads/{version}.tar.gz"
        }
    }
}
```

Every reference or copy from an unpinned collection downloads the branch again
and takes whatever it holds at that moment. There is no `sha256`, so nothing
checks what arrived. Audit enforcement refuses both; without enforcement a
reference runs that unverified content, while a copy lands in the tracked tree
for review. Tagging the collection instead verifies every fetch, at the cost of
versioning every module in it together.

`unpinned` is the third answer beside `renovate` and `manual` and excludes
both: it says the ref moves on its own. It cannot be combined with a `sha256`,
which a moving ref would break the moment it moved, and no other holder of a
pin may carry it at all — the build fetches an out-of-tree module and runs it
as root without anyone reading it first, so its hash is not optional. A pin
with neither a `sha256` nor `unpinned` is still an error, so a hash that is
dropped or mistyped is reported rather than quietly becoming trust in whatever
answers the URL.

`tect check` names every unpinned collection above its counts. It is not an
error; it is the one thing that stops the repository being reproducible, and
it is worth being reminded of.

`audit` is the posture, and only the posture. Every provenance fact is recorded
whether or not it is declared: what a module hashed to, where it was imported
from, what the base tag resolved to, what commit a cloned asset was taken at.
`enforce #true` decides which of those being missing or not matching stops the
run.

```kdl
audit {
    enforce #true
}
```

Under enforcement these are errors rather than read-outs: importing from a
collection that follows a moving ref with no `sha256`, a module whose content no
longer matches the record it was imported with, a base tag that will not resolve
to a manifest digest, and a repository at no commit. Without it the same
repository checks and builds clean and the same facts are written down, which is
what stops a built artifact ever implying an audit it did not get.

One diagnostic is on either way: a module whose `module.sh` or `finalize.sh`
reaches the network with no `asset` declaring what it pulls. An undeclared fetch
is the one build input no record can describe after the fact, because nothing
says what it should have been.

`seed` nominates the image a new repository may start from, and `generate`
writes it to `generated/seed.kdl`.

```kdl
seed "workstation" collection="tectonic-os"
```

The seed carries the base, the module list and the collections those modules
come from, and nothing about this repository: no name, no URL, no owner. Each
module is named by the collection it is fetched through, which is why
`collection` is required and has to be one of the collections in `sources`: a
repository is seedable only if it publishes its own `modules/` as a collection
too. An image listing a module nothing can import that way is reported, since
the seed of it would leave a new repository unbuildable.

<!-- schema: repo -->

| Node | Takes | Meaning |
| --- | --- | --- |
| `schema-version` | a number, at most one | The schema release this repository is written against, which picks the reader. |
| `name` | a string, exactly one | What the repository calls itself, whatever the directory holding it is called. |
| `default-image` | a string, at most one | The image a command given no image answers about, and a build with no target builds. |
| `pr-image` | a string, at most one | The image a pull request builds. |

### `tect-version`

The tect release this repository is built with, which `scripts/tect.sh` fetches for the build.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `sha256=` | a string | The release tarball's sha256, which `scripts/tect.sh` holds the download to. Absent, the script checks against the checksum fetched beside the tarball, which proves the download and nothing more. |

### `seed`

The image this repository publishes a declaration of, for a new repository to start from.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `collection=` | a string, required | The collection this repository publishes its own modules as, which is what names them in the seed. |

### `workflows`

The CI `tect generate` writes into .github/workflows/, named by file stem. One this does not name is not written.

*at most one, never empty*

| Property | Value | Meaning |
| --- | --- | --- |
| `at=` | a string | The hour and minute the daily build runs, UTC. Every other schedule is an offset from it. |
| `publish=` | a string | When images publish. `scheduled` moves publishing off pushes while keeping the daily build. |
| `scan=` | a string | When image scans run. `scheduled` moves them off pushes; scheduled publishing does too because scans consume published images. |

| Node | Takes | Meaning |
| --- | --- | --- |
| `<name>` |  | One workflow, named by the node. |

### `sources`

The module collections `tect import module` and `tect copy module` resolve against.

*at most one, never empty*

#### `<name>`

One module collection, named by the owner its references use.

*a string*

Also holds [`pin`](pins.md#pin).

### `manifest`

Whether a build stamps the generated manifest onto the image as an OCI label.

*at most one, never empty*

| Node | Takes | Meaning |
| --- | --- | --- |
| `label` | `#true` or `#false`, at most one | Whether the build stamps `org.tectonic.manifest` with the path to the baked manifest file. |

### `audit`

How strictly the provenance facts are held. Every one of them is recorded either way; this decides only which of them is fatal.

*at most one, never empty*

| Node | Takes | Meaning |
| --- | --- | --- |
| `enforce` | `#true` or `#false`, at most one | Whether a provenance fact that is missing or does not match stops the run. Off, it is reported and the run carries on. |

<!-- /schema: repo -->
