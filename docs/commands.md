# The commands

`tect --help` lists what can run where you are standing. This is the whole
surface, including the parts the help does not offer because their audience is
a script or a build layer.

The repository is the nearest directory at or above the working directory
holding a `repo.kdl`, or `--root`. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.

A command a person reads prints `Tectonic v<version>` and a blank line before
anything else. The commands under "For scripts" and "Inside a build layer"
print no banner, because their stdout is what something else reads.

A failure reads:

    Tectonic v0.0.0

    Error: modules/firefox is already there.

    You can find the available commands by typing 'tect' or 'tect --help'

Usage is printed in place of that last line when the invocation was what was
wrong: an unknown command, a flag the command does not read, a bad argument, no
repository.

## Flags, and what is asked for

Every command takes a flag for everything it needs.

- All of them supplied: nothing is asked, and nothing opens.
- One missing, and stdin is a terminal: it is asked for.
- One missing, and stdin is not a terminal: the command fails naming the flag.

`--no-tui` forces the third case, so a script gets the same behaviour whether
or not it happens to have a terminal.

A yes or no step has no flag of its own. The flag that answers it is the
answer: `--image desktop` on `create repo` means yes and names the image, and
its absence under `--no-tui` means no. A value that may repeat has a flag that
repeats with it, so `--image` on `create module` names every image the module
is listed in.

Every question is asked before anything is written, and whatever a question
depends on is read before it is asked, so a name already taken or an image that
is not declared is refused with nothing left behind. What is written afterwards
is one line per step, in the order they were written, and a step that fails
stops the command: what earlier steps wrote stays, and each of those steps is
also a command of its own to finish the run with.

## Starting a repository

### `create repo [name]`

Writes a new repository: `repo.kdl`, the module directory, and the build
scripts, shell helpers, disk config and workflows the tool ships and a
repository does not carry. Into `--root`, else a directory named for the
repository.

The `repo.kdl` it writes declares `tectonic-os/modules` in `sources`, so
`import module` works in a repository that has just been created. That
collection is declared `unpinned`: it is followed at its branch head rather
than at a tag, so every import of it takes whatever the branch holds then and
no `sha256` checks what arrived. It is scaffolded that way because tagging the
collection would version every module in it together. Delete the block, or
replace it with a tagged and hashed pin, if that trade is not one you want;
`docs/schema.md` says what each spelling costs.

That block is not compiled in. It is `repo.sources.kdl` in the assets
directory, which is the release tarball's `assets/` beside the binary, or
`~/.local/share/tectonic/assets/` on an installed copy. Editing it changes
what every repository created afterwards declares, and deleting it scaffolds
no `sources` at all, which is a valid repository that imports from nothing
until you write one. It is the only asset that does not land in the
repository: it is spliced into `repo.kdl` instead.

It asks for the name, then whether the images are to be built on a schedule.
That is what a remote is for, so yes asks where the repository is hosted,
`--host`, which is `github.com` unless the picker or the flag names another,
and who owns it there, `--owner`, your account or org and not `tectonic-os`.
The two compose into the origin, which is what the remote and every URL an
image carries are built from.

On github it then offers to create the repository, and where `gh` is not
installed or not logged in it says so and what to do about it rather than
offering. Last it offers to write an image, which is `create image` run in
place, so `--image` and `--base` are that command's flags.

The tree, the git repository, the image and the remote are written in that
order, each adding to the one before. `git init` is the tool's, and it is all
of git that is: the first commit, the remote and the push are yours, and the
closing lines are the commands for them.

A repository does not nest, and one inside another is refused.

## Working in a repository

### `create image [name]`

Writes `<image-id>.kdl` at the repository root, where the id is the machine
name the name derives, so "My Desktop" writes `my-desktop.kdl`. Every root
`.kdl` but `repo.kdl` is one image.

Asks for the name, which defaults to what the repository is called, so a
repository whose first image is the repository itself takes it by pressing
return. Standing in one, the name it is called is the name of the directory it
sits in. Then it offers the bases the tool knows; `--base`. Choosing
none asks for a reference instead, defaulting to
`quay.io/fedora/fedora-bootc:44`, so any image can be built on. A base out of
the catalog writes its family and what it already ships into `base`, and a
base from outside it asks for the family and writes no `provides`: nothing
knows what an unknown image carries, so `check` reports what no module
satisfies.

The catalog is a seed the tool ships with, which is why the picker works with
nothing fetched and no network, and every collection `repo.kdl` declares in
`sources` extends it with a `bases.kdl` at its root. A collection describing a
base the tool ships an entry for wins, which is how a stale entry is corrected
without a tool release, and `check` names the collection wherever the two
differ. A base two collections describe is an error. Nothing is fetched to read
one: a collection that is not on this machine extends nothing, and the seed is
what the picker offers.

`url` and `issues-url` are the repository's own, not the image's: every image
in a repository is published out of one remote, so the id in them is the
repository's. `create repo` composes it out of `--host` and `--owner`; a
`create image` standing in a repository takes it off an image already there,
and `--owner` names it where no image carries one. A repository that declared
no origin writes neither field, and a second image into one matches.

A repository with no image at all is a valid state. `check` says what is
missing rather than failing, and `generate` says there is nothing to write.

A second image makes a bare build ambiguous, so writing one into a repository
that declares no `default-image` appends one naming the image already there:
what a bare build built before is what it still builds. The line is appended
and nothing else in `repo.kdl` moves. A repository that already declares a
default keeps it.

Nothing reports a missing default until a command has to pick an image with
nothing naming one, so `check`, `generate` and anything given an image of its
own never see it.

### `create module [name]`

Writes `modules/<name>/module.kdl`. The name may be a path, so
`create module apps/firefox` writes `modules/apps/firefox/module.kdl`.

Asks whether the module installs packages and, if so, which, scaffolding them
as `packages { fedora "..." }`; `--pkg`, repeatable. `--with verb=value`, also
repeatable, writes one more line into the manifest, so
`--with provides=browser` declares the capability. `check` holds whatever is
written to the schema like any other manifest.

It then offers to list the module in the images it is to be built into. Every
image the repository declares is numbered, several of them are an answer, given
on the one line as `1 3` or `1,3`, and so is none: having a module in the
repository and listing it in an image are different decisions, which is what
makes a repository with several images work. `--image` is repeatable and says
the same, so `--image desktop --image server` lists it in both and each is one
line of what the run wrote. A name the repository does not declare is refused
before the module is written.

### `import module [name]`

Copies one module out of a source collection into `modules/<owner>/<name>`,
where `<owner>` names the collection `repo.kdl` declares it in. A bare name is
searched for in every collection; `<owner>/<name>` picks between two that both
have it, and so does the prompt when there is a terminal.

With no name it lists every module the collections hold, each with its
description and what it requires, and asks which one.

Then it offers to list the module in the images it is to be built into, the
same offer `create module` ends with; `--image`, repeatable.

This is the only command that fetches a collection. A pinned one is downloaded
and verified once and kept, and everything else in the tool reads whatever is
already on disk, which is why the base picker costs no network. An `unpinned`
collection has no hash to key that cache on, so it is downloaded again every
time this command runs, unverified: the module lands in your tree as a copy
you read and commit, and that reading is what stands in for the missing hash.

### `create key <kind>`

Generates one of the keys the repository's modules declare. The argument names
which; everything else comes out of the module's `key` node, so the tool holds
no path of its own. With no argument the declared kinds are listed and one is
asked for.

The public half goes into the files/ overlay of the module that declared it,
which is where the build reads it from and what makes the path a contract path
the module provides. `--module` picks between two modules declaring the same
kind, and so does the prompt when there is a terminal. The private half is
written at the repository root, under the name the declaration gives it.

No module here declaring the kind is not a missing file but a missing module,
so the failure names the one in the declared collections that does, the
collection it comes from, and the `import module` line that fetches it. Where
no collection has one either, it says what declaration to write.

The generators are the tool's and closed, and each carries what follows a key
it wrote:

- `cosign` writes the keypair a published image is signed with and a signature
  policy verifies updates against. `cosign` has to be installed. The key
  carries no password, which is what the build workflow decrypts it with; set
  it as the `SIGNING_SECRET` repository secret.
- `openssl profile="module-signing"` writes the Secure Boot certificate the
  build signs kernel modules and the kernel with, at the declared `bits` and in
  the declared `format`; DER is what `sign-file` and `mokutil` read. `--cn` is
  the certificate's common name, which is what the enrolment prompt shows, and
  it defaults to the repository directory's name. Set the private half as the
  `MOK_PRIVKEY` repository secret; `$MOK_KEY_PATH` points a local build at it.
  Every machine that boots the image enrols the certificate once with
  `mokutil --import`, and until it does the modules signed with it will not
  load.

A key already there is never replaced, because the private half cannot be
recovered. The zero-byte file a module ships as a placeholder is not a key. The
private halves are named in the `.gitignore` the scaffolding writes; a
repository whose `.gitignore` does not cover one is told so rather than edited.

### `check`

Reads every manifest and reports every problem at the line that caused it,
with the counts on the last line: images, modules, flavours, and how many
listed modules the base already provides and nothing therefore builds. Above
them it names every base a declared collection describes differently from the
entry the tool ships, since the collection's is what an image is scaffolded
from, and every collection declared `unpinned`, which is what stops the
repository being reproducible. Neither is an error and neither changes the
exit code.

### `generate`

Writes, under `generated/`, the Containerfile for each image, the per-module
build scripts, both renderings of the capability graph, `plan.json` (the same
document `plan --json` prints), and, when `repo.kdl` nominates a seedable
image, the `seed.kdl` a new repository starts from. Lists what it wrote. The
directory is cleared first, so an image or a module that is gone leaves with
its files. `generated/` is tracked; `out/` is scratch and ignored. The
Containerfile bakes `plan.json` into the image at
`/usr/share/tectonic/manifest.json`, so a built image can answer what it is
made of without the repository that built it.

### `build [target]`

Fetches the pinned modules, runs `verify` as the drift gate, then execs the
container backend with the argv the plan derives: the Containerfile, the build
args, the tags, the secrets and the layer cache references. The default target
when none is named; a target is `<image>/<flavour>`, and the flavour half is
`none` for the ungated set.

    --kernel <name>       the KERNEL build arg, which the Containerfile decides
                          when nothing sets it
    --tag <ref>           tag the result; repeatable, and $TAGS adds to it
    --secret <id>=<path>  mount <path> as the build secret <id>; repeatable
    --backend <name>      buildx or buildah, else $BUILD_BACKEND, else buildah
    --oci-output <path>   write an OCI archive instead of loading the image
    --cache-to            export the layer cache to the registry cache repo
    --no-cache-from       do not import it

`$LABELS` adds OCI labels the way `$TAGS` adds tags, `$IMAGE_VERSION` is
stamped into the image and defaults to today in UTC, and `$MOK_KEY_PATH` is
shorthand for `--secret mok_privkey=<path>`.

Nothing is regenerated here. A build proves the committed files are current.

### `section [image]`

Prints the generated Containerfile module section for one image, the default
image when none is named. This is the part `generate` splices into the
skeleton.

### `graph [--format md|json]`

Prints the default image's capability graph: what provides what, what requires
it, what only orders against it, and what the base already carries. Markdown
holding a mermaid diagram, unless `json` is asked for.

## For scripts

None of these is in the help a person reads. They are the contract the build
runs against rather than something to read.

### `plan [--json]`

Every fact this repository derives, as one JSON document: the images, each
image's targets, and what each target is made of. Read a field out of it
rather than deriving anything from a name. Nothing in the build derives a
name, a target, a tag or an order; the tool does it once, here.

### `verify`

Re-emits every artifact and byte-compares it against what is committed under
`generated/`, naming what differs, what is missing, and anything under
`generated/` that nothing emits. It is the drift gate, and it runs before
every build.

### `summary [target]`

What one target is made of, as a markdown table: every module it builds, with
its description and the options it resolved. The default target when none is
named. This is what a build writes into its job summary.

### `sbom [target]`

The pinned payloads one target carries, as SPDX packages and the relationships
that describe them. A scan of the built image cannot see where a downloaded
asset came from, so this is merged into the SBOM the scan produces.

### `fetch modules`

Fetches every out-of-tree module the images pin, verifies it against the hash
they pin it at, and puts it under `modules/.remote/` where the resolver looks
for it. A tree already at its pin is left alone; one no image pins any more is
removed. What each fetch hashed to is stamped under `out/`, so nothing under
`modules/` is tool-written state.

It reads the declarations rather than the resolved plan, so it runs before the
modules it fetches can be read.

### `registry namespace` and `registry ref [--target T] [--tag X]`

Where an image publishes. The namespace is `$IMAGE_REGISTRY`, which CI sets
from the workflow context, else `ghcr.io/<owner>` read off the github origin
remote. `ref` joins it to the name the target publishes under, at `--tag`, else
`$DEFAULT_TAG`, else latest; the ungated target when none is named.

Reading the remote is a read. The tool still writes nothing to git.

## Inside a build layer

These read the image around them rather than a repository, run where the
binary is mounted into a build, and are never advertised.

### `os-release`

Writes the image identity the build ARGs carry into `/usr/lib/os-release`.

### `fetch <what> <url> <sha256> [target] [extra...]`

Downloads, verifies against the hash, and places it: `file` keeps it, `tree`
unpacks it, `bin` installs one executable, `rpm` installs the package.

### `validate-image`

Every check a built image has to pass. The build passes it the preset files the
enabled modules' overlays ship, and it fails on any of them the image does not
have.
