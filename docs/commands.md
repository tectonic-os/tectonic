# Commands

`tect --help` lists the commands a person runs: `create repo`, and the ones
that need a repository. The two families at the end of this file — what a build
runs against, and what runs inside a build layer — are the contract rather than
the help, and are not in that list.

`tect` with nothing after it opens a picker of the same list where the output
is a terminal, and prints the list where it is not. A verb with no noun —
`tect create`, `tect import`, `tect registry`, `tect set` — opens a picker of
its nouns.
Leaving a picker is not an error: it exits 0 having done nothing.

The repository is the nearest directory at or above the working directory
holding a `repo.kdl`, or `--root <dir>`. Data goes to stdout, diagnostics to
stderr. Exit 1 is the invocation, exit 2 the repository.

`docs/schema.md` is the reference for what the manifests hold.

## Flags and prompts

Every command takes a flag for everything it needs.

- All of them supplied: nothing is asked, and nothing opens.
- One missing, and stdin is a terminal: it is asked for.
- One missing, and stdin is not a terminal: the command fails naming the flag.

`--no-tui` forces the third case, so a script behaves the same whether or not
it has a terminal.

A yes or no step has no flag of its own. The flag that answers it is the
answer: `--image desktop` on `create repo` means yes and names the image, and
its absence under `--no-tui` means no. A repeatable flag answers a step that
takes several values.

Every question is asked before anything is written, so a name already taken or
an image that is not declared is refused with nothing left behind. A step that
fails stops the command: what earlier steps wrote stays, and each of those
steps is a command of its own to finish the run with.

`create repo`, `create image`, `create module` and `import module` end with a
tree of the files they wrote, rooted at the repository, each leaf carrying a
phrase saying what it is for. `create key` names its two halves instead: one of
them is a private key outside the repository, which no tree rooted there holds.

`--root <dir>` is accepted by every command below. A flag a command does not
read is an error, not a silent no-op.

## Starting a repository

### `create repo [name]`

Writes a new repository with the following into the current directory unless a `--root`
location is defined in the command:
- A `repo.kdl` file
- A module directory
- A build scripts directory
- A shell helpers directory
- A disk config directory
- A workflows directory for CI

#### Flags:
    --host <domain>   where the repository is hosted; github.com by default
    --owner <name>    your account or org for your repo host
    --image <name>    optionally write an initial image (`create image` can create this later)
    --base <ref>      a bootc image that this image is based on

#### Notes:
- `--host` and `--owner` compose into the origin every image URL is built from.
  They are asked for only if you say the images build on a schedule.
- On github it offers to create the repository for you, and says what to do
  instead where `gh` is missing or logged out.
- `git init` is all of git the tool does. The first commit, the remote and the
  push are yours, and the closing lines are the commands for them.
- The `repo.kdl` it writes declares `tectonic-os/modules` in `sources`, so
  `import module` works immediately. That collection is `unpinned`: it follows a
  branch head, so every import takes whatever the branch holds then and no
  `sha256` checks what arrived. Delete the block or replace it with a tagged and
  hashed pin if that trade is not one you want.
- A repository does not nest, and one inside another is refused.

## Working in a repository

### `create image [name]`

Writes one image file, `<image-id>.image.kdl`, at the repository root. The id
is the machine name your name derives, so "My Desktop" writes
`my-desktop.image.kdl`. A root `.kdl` is an image only when it is named
`image.kdl` or ends in `.image.kdl`; anything else is reported rather than read.

Asks you for:
- The image name, which defaults to what the repository is called
- The base it builds on, picked from the catalog or given as any bootc reference

#### Flags:
    --base <ref>      a bootc image that this image is based on; skips the picker
    --owner <name>    your account or org, where no image already carries one

#### Notes:
- A base from the catalog writes its family and what it already ships into
  `base`. A base from outside the catalog asks you for the family and writes no
  `provides`, so `check` reports what no module satisfies.
- `url` and `issues-url` are the repository's, not the image's, since every
  image is published out of one remote. A repository that declared no origin
  writes neither, and a second image matches the first.
- Writing a second image into a repository that declares no `default-image`
  appends one naming the image already there, so a bare build builds what it
  built before. Nothing else in `repo.kdl` moves.

### `create module [name]`

Writes one module manifest, `modules/<name>/module.kdl`. The name may be a
path, so `create module apps/firefox` writes
`modules/apps/firefox/module.kdl`.

Asks you for:
- Whether the module installs packages, and which ones
- Which images the module is listed in, if any

#### Flags:
    --pkg <name>          a package the module installs; repeatable
    --with verb=value     one more line in the manifest, such as `--with provides=browser`; repeatable
    --image <name>        list the module in this image; repeatable

#### Notes:
- Packages are scaffolded under the repository's own family, as
  `packages { fedora "..." }`, `debian` or `ubuntu`. The generated build layer
  runs `dnf5 install -y` on Fedora and `apt-get update` / `install -y` /
  `clean` on Debian and Ubuntu, per matching group.
- `enablerepo=` is Fedora-only, and naming it on a Debian or Ubuntu group is a
  `check` diagnostic.
- Anything `--with` writes is held to the schema like the rest of the manifest.
- You are asked which of the declared images list the module, and several are an
  answer: space toggles one on a terminal, and `1 3` or `1,3` answers the
  numbered list where there is not one. So is none: having a module and listing
  it in an image are different decisions. A name the repository does not declare
  is refused before the module is written.

### `import module [name]`

Copies one module out of a source collection into `modules/<owner>/<name>`,
where `<owner>` names the collection that holds it.

Asks you for:
- Which module, listing every one the collections hold with its description and
  what it requires, when no name is given
- Which images the module is listed in, if any

#### Flags:
    --image <name>    list the module in this image; repeatable

#### Notes:
- A bare name is searched for in every collection. `<owner>/<name>` picks
  between two collections that both have it.
- This is the only command that fetches a collection. Everything else reads what
  is already on disk, which is why the base picker costs no network.
- A pinned collection is downloaded and verified once and kept. An `unpinned`
  one has no hash to cache on, so it is downloaded again every time,
  unverified.

### `create key <kind>`

Generates one of the keys the repository's modules declare, and writes:
- The public half, into the `files/` overlay of the module that declared it
- The private half, at the repository root under the name the declaration gives
  it

Asks you for:
- Which kind, listing the kinds the modules declare, when no argument is given

#### Flags:
    --module <name>   which module, where two of them declare the same kind
    --cn <name>       the certificate common name; the repository directory name by default

#### Notes:
- The private halves are covered by the scaffolded `.gitignore`. A repository
  whose `.gitignore` does not cover one is told rather than edited.
- An existing key is never replaced, because the private half cannot be
  recovered. A zero-byte placeholder is not a key.
- No module here declaring the kind is a missing module, not a missing file, so
  the failure names one in the declared collections that has it and the
  `import module` line that fetches it.

The generators are closed, and each is one of:
- `cosign` writes the keypair a published image is signed with and a policy
  verifies updates against. `cosign` has to be installed. The key carries no
  password; set it as the `SIGNING_SECRET` repository secret.
- `openssl profile="module-signing"` writes the Secure Boot certificate the
  build signs the kernel and its modules with, at the declared `bits` and
  `format`. DER is what `sign-file` and `mokutil` read. Set the private half as
  the `MOK_PRIVKEY` repository secret; `$MOK_KEY_PATH` points a local build at
  it. Every machine enrols the certificate once with `mokutil --import`, and
  until it does the modules signed with it will not load.

### `set workflows`

Chooses the CI this repository generates, and writes the choice into the
`workflows` block of `repo.kdl`. `tect generate` is what then writes the files.

Asks you for:
- Which of the shipped workflows to generate, opening with the ones already
  declared, and drawing what each of them needs where the repository cannot run
  it
- What time the daily build runs, UTC, where any of the chosen ones has a
  schedule

#### Notes:
- There are no toggle flags. The declaration file was always the interface, so
  with nobody to ask this says to edit `repo.kdl` rather than naming a flag
  that would be a second way to write the same line.
- Leaving the picker changes nothing. Choosing nothing takes the block away,
  and a repository declaring no block generates no CI.
- `build-disk` and `smoke-test` need a fedora image, because the image builder
  relabels its buildroot with SELinux and builds no disk otherwise.
  `kernel-freshness` needs a module taking a `KERNEL` build arg. One whose
  basis is absent is drawn with the reason and refused.
- Every schedule is an offset from the daily build, so moving one value moves
  all of them. A cron is the one value the emitter writes into a workflow file
  rather than putting in `plan.json`, because the forge reads it out of the
  file before any job exists.

### `check`

Reads every manifest and reports every problem at the line that caused it, then
the counts on the last line: images, modules, flavours, and how many listed
modules the base already provides.

#### Notes:
- Above the counts it names every base a collection describes differently from
  the tool's own entry, and every collection declared `unpinned`.
- Neither of those is an error, and neither changes the exit code.

### `generate`

Writes the build files and lists what it wrote:
- The Containerfile for each image
- The per-module build scripts
- Both renderings of the capability graph
- `plan.json`
- `seed.kdl`, where `repo.kdl` nominates a seedable image
- Every workflow the `workflows` block names, under `.github/workflows/`

#### Notes:
- `generated/` is cleared first, so an image or module that is gone leaves with
  its files. A workflow is removed by name instead: one the tool does not ship
  is the repository's own and is left where it is.
- The workflow bodies are shipped verbatim, with the declared schedules
  substituted and the kernel build input kept only where a listed module takes
  one. A tool upgrade re-syncs them by regeneration, so nothing is ever copied
  by hand.
- `generated/` is tracked; `out/` is scratch and ignored.
- The Containerfile bakes `plan.json` into the image at
  `/usr/share/tectonic/manifest.json`, so a built image can answer what it is
  made of.
- The base tag is resolved to a manifest digest once and passed down as the
  `BASE` build argument, which the generated `FROM` reads. `$BASE` in the
  environment is taken as already resolved, so CI that stamped
  `org.opencontainers.image.base.digest` and the build record agree rather than
  resolving a moving tag twice.

### `build [target]`

Builds one target: fetches the pinned modules, runs `verify` as the drift gate,
then execs the container backend. A target is `<image>/<flavour>`, and the
ungated set is named by the bare image id. The default target when none is
named.

#### Flags:
    --target <t>          the target, where the positional argument is not used
    --kernel <name>       the KERNEL build arg
    --tag <ref>           tag the result; repeatable, and $TAGS adds to it
    --secret <id>=<path>  mount <path> as the build secret <id>; repeatable
    --backend <name>      buildx or buildah, else $BUILD_BACKEND, else buildah
    --oci-output <path>   write an OCI archive instead of loading the image
    --cache-to            export the layer cache to the registry cache repo
    --no-cache-from       do not import the layer cache

#### Notes:
- `$LABELS` adds OCI labels the way `$TAGS` adds tags, and `$IMAGE_VERSION` is
  stamped into the image, defaulting to today in UTC.
- Nothing is regenerated here. A build proves the committed files are current.

### `section [image]`

Prints the generated Containerfile module section for one image, the default
image when none is named.

### `graph`

Prints the default image's capability graph: what provides what, what requires
it, what only orders against it, and what the base already carries.

#### Flags:
    --format <md|json>    markdown holding a mermaid diagram by default, or json

## For scripts

None of these is in the help a person reads. They are the contract the build
runs against.

### `plan [--json]`

Prints every fact this repository derives, as one JSON document: the images,
each image's targets, and what each target is made of. Read a field out of it
rather than deriving anything from a name.

### `verify`

Re-emits every artifact and byte-compares it against what is committed under
`generated/`, naming what differs, what is missing, and anything under
`generated/` that nothing emits. It runs before every build.

### `summary [target]`

Prints what one target is made of, as a markdown table: every module it builds,
with its description and the options it resolved. This is what a build writes
into its job summary.

### `sbom [target]`

Prints the pinned payloads one target carries, as SPDX packages and the
relationships that describe them. A scan of the built image cannot see where a
downloaded asset came from, so this is merged into the SBOM the scan produces.

### `fetch modules`

Fetches every out-of-tree module the images pin, verifies it against the hash
they pin it at, and puts it under `modules/.remote/`.

#### Notes:
- A tree already at its pin is left alone; one no image pins any more is
  removed.
- It reads the declarations rather than the resolved plan, so it runs before the
  modules it fetches can be read.

### `scap <arf.xml>`

Reads one scan's report against the datastream it was produced with, and prints
what the two of them say about the target, as markdown: what the modules
claimed and what was measured for each, what the image scores against every
profile the datastream carries, and what stopped passing since the last scan.

#### Flags:
    --target <t>          the target, else the ungated one
    --datastream <f>      the SSG content, else the one `scap content` names
    --baseline <f>        the last scan's pass set, read then rewritten

#### Notes:
- The mapping from a benchmark number to a rule is the datastream's own, over
  every `reference`, `ident` and `version` it carries, and the first rule in
  document order wins. A number that maps to nothing is a failure of the
  declaration rather than of the image.
- A claimed rule the image fails names the module that claimed it, and, where
  another module replaced a file the claimant ships, names that too: the claim
  is not contradicted, the composition defeats it.
- `image { conforms }` is measured and reported, never enforced. A profile
  nothing in the datastream carries is a finding, and the ones it does carry
  are listed.
- `--baseline` is the ratchet. The file is read before it is written, so a rule
  that passed the last scan and does not now is a finding, and every run leaves
  the current pass set behind as the next floor. One deliberate regression is
  one red run rather than a file somebody has to go and delete.
- Findings are fatal only under `audit { enforce #true }`, like every other
  audit fact, and the report goes to stdout either way.

### `scap content`

Prints the datastream the target is measured with, and nothing at all when it
declares neither a `satisfies` nor a `conforms`, which is an image asking not
to be scanned. This is what the scan job gates on.

#### Flags:
    --target <t>          the target, else the ungated one

### `registry namespace`

Prints where images publish: `$IMAGE_REGISTRY`, else `ghcr.io/<owner>` read off
the github origin remote.

### `registry ref`

Prints the full reference one target publishes under, joining the namespace to
the target's name and tag. The ungated target when none is named.

#### Flags:
    --target <t>          the target, else the ungated one
    --tag <x>             the tag, else $DEFAULT_TAG, else latest

### `why <module> [--format md|json]`

One module's trust read-out: which targets build it, what it provides and who
requires that, what it requires and what provides it, what it claims to
harden, and where every byte of it came from — the collection it was imported
from and at what pin, whether it has been edited since, what it fetches, and
whether it enables a third-party package repository.

#### Flags:
    --format <md|json>    markdown, the default, or JSON

#### Notes:
- It answers two ways from one renderer. In a repository it reads the resolved
  plan. With no `repo.kdl` anywhere above it, it reads
  `/usr/share/tectonic/manifest.json` and `/usr/share/tectonic/build.json`
  instead, which every built image carries, so a live host can ask what it is
  running and where that came from.
- A module edited since it was imported is said so plainly and is not an error.
  Forking one is legitimate; what the record buys is that the fork is visible
  rather than silent. `audit { enforce #true }` is what makes it fail.
- A name nothing declares lists the ones that are declared.
- There is no grammar for a `repo` file, so this points at it and prints the
  URLs it found rather than claiming to have understood it.

## Inside a build layer

These read the image around them rather than a repository, and run where the
binary is mounted into a build.

### `os-release`

Writes the image identity the build ARGs carry into `/usr/lib/os-release`.

### `build-record`

Writes `/usr/share/tectonic/build.json`, the record of what the build
**resolved**, where the baked `manifest.json` beside it is what the repository
**declared**. It carries the digest the base tag resolved to, the commit each
cloned asset's selector named, the source commit, the tect release, the target,
the module content hashes, whether enforcement was on, and `verified: null`
until something has checked the claims.

Nothing under `generated/` holds it, so `verify` never sees it: a daily
changing resolution in a committed file would fail the drift gate every
morning, which is why the resolution is a second document rather than a field
of the first.

### `fetch <what> <url> <sha256> [target] [extra...]`

Downloads one payload, verifies it against the hash, and places it by what it
is:
- `file` keeps it
- `tree` unpacks it
- `bin` installs one executable
- `rpm` installs the package, on an rpm family
- `deb` installs the package, on a deb family

### `validate-image`

Runs every check a built image has to pass. The build passes it the preset files
the enabled modules' overlays ship, and it fails on any the image does not have.
