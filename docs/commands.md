# Commands

`tect --help` lists what can run.
As `tect` provides the tools for both the user facing CLI and during the build,
This will display all commands available.

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

Writes one image file, `<image-id>.kdl`, at the repository root. The id is the
machine name your name derives, so "My Desktop" writes `my-desktop.kdl`. Every
root `.kdl` but `repo.kdl` is one image.

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
- Every declared image is numbered when you are asked which list the module in.
  Several are an answer on one line, as `1 3` or `1,3`, and so is none: having a
  module and listing it in an image are different decisions. A name the
  repository does not declare is refused before the module is written.

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

### `check`

Reads every manifest and reports every problem at the line that caused it, then
the counts on the last line: images, modules, flavours, and how many listed
modules the base already provides.

#### Notes:
- Above the counts it names every base a collection describes differently from
  the tool's own entry, and every collection declared `unpinned`.
- Neither of those is an error, and neither changes the exit code.

### `generate`

Writes the build files under `generated/` and lists what it wrote:
- The Containerfile for each image
- The per-module build scripts
- Both renderings of the capability graph
- `plan.json`
- `seed.kdl`, where `repo.kdl` nominates a seedable image

#### Notes:
- The directory is cleared first, so an image or module that is gone leaves with
  its files.
- `generated/` is tracked; `out/` is scratch and ignored.
- The Containerfile bakes `plan.json` into the image at
  `/usr/share/tectonic/manifest.json`, so a built image can answer what it is
  made of.

### `build [target]`

Builds one target: fetches the pinned modules, runs `verify` as the drift gate,
then execs the container backend. A target is `<image>/<flavour>`, where the
flavour half is `none` for the ungated set. The default target when none is
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

### `registry namespace`

Prints where images publish: `$IMAGE_REGISTRY`, else `ghcr.io/<owner>` read off
the github origin remote.

### `registry ref`

Prints the full reference one target publishes under, joining the namespace to
the target's name and tag. The ungated target when none is named.

#### Flags:
    --target <t>          the target, else the ungated one
    --tag <x>             the tag, else $DEFAULT_TAG, else latest

## Inside a build layer

These read the image around them rather than a repository, and run where the
binary is mounted into a build.

### `os-release`

Writes the image identity the build ARGs carry into `/usr/lib/os-release`.

### `fetch <what> <url> <sha256> [target] [extra...]`

Downloads one payload, verifies it against the hash, and places it by what it
is:
- `file` keeps it
- `tree` unpacks it
- `bin` installs one executable
- `rpm` installs the package

### `validate-image`

Runs every check a built image has to pass. The build passes it the preset files
the enabled modules' overlays ship, and it fails on any the image does not have.
