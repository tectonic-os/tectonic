# The commands

`tect --help` lists what can run where you are standing. This is the whole
surface, including the parts the help does not offer because their audience is
a script or a build layer.

The repository is the nearest directory at or above the working directory
holding a `repo.kdl`, or `--root`. Data goes to stdout and diagnostics to
stderr; exit 1 is the invocation, exit 2 the repository.

## Flags, and what is asked for

Every command takes a flag for everything it needs.

- All of them supplied: nothing is asked, and nothing opens.
- One missing, and stdin is a terminal: it is asked for.
- One missing, and stdin is not a terminal: the command fails naming the flag.

`--no-tui` forces the third case, so a script gets the same behaviour whether
or not it happens to have a terminal.

A yes or no step has no flag of its own. The flag that answers it is the
answer: `--image desktop` on `create repo` means yes and names the image, and
its absence under `--no-tui` means no.

## Starting a repository

### `create repo [name]`

Writes a new repository: `repo.kdl`, the module directory, and the build
scripts, build phases, disk config and workflows the tool ships and a
repository does not carry. Into `--root`, else a directory named for the
repository.

It asks for the name, then the owner, which is your account or org on github
and not `tectonic-os`; `--owner`. It then offers to create an image, which is
`create image` run in place, so `--image` and `--base` are that command's
flags. Last, and only when `gh` is installed, it offers to create the
repository on github. Nothing about that is a prerequisite: the tree is
already written, and `git init`, the first commit and the push are yours.

A repository does not nest, and one inside another is refused.

## Working in a repository

### `create image [name]`

Writes `<image-id>.kdl` at the repository root, where the id is the machine
name the name derives, so "My Desktop" writes `my-desktop.kdl`. Every root
`.kdl` but `repo.kdl` is one image.

Asks for the name and the base image, which defaults to
`quay.io/fedora/fedora-bootc:44` and accepts any reference; `--base`.

A repository with no image at all is a valid state. `check` says what is
missing rather than failing, and `generate` says there is nothing to write.

A second image makes a bare build ambiguous, which `check` reports: name one
of them in `default-image` in `repo.kdl`.

### `create module [name]`

Writes `modules/<name>/module.kdl`. The name may be a path, so
`create module apps/firefox` writes `modules/apps/firefox/module.kdl`.

Asks whether the module installs packages and, if so, which, scaffolding them
as `packages { fedora "..." }`; `--pkg`, repeatable. `--with verb=value`, also
repeatable, writes one more line into the manifest, so
`--with provides=browser` declares the capability. `check` holds whatever is
written to the schema like any other manifest.

It then offers to list the module in an image. It asks which one even when
there is only one, and none is an answer: having a module in the repository
and listing it in an image are different decisions, which is what makes a
repository with several images work.

### `import module [name]`

Copies one module out of a source collection into `modules/<owner>/<name>`,
where `<owner>` names the collection `repo.kdl` declares it in. A bare name is
searched for in every collection; `<owner>/<name>` picks between two that both
have it, and so does the prompt when there is a terminal.

With no name it lists every module the collections hold, each with its
description and what it requires, and asks which one.

Then it offers to list the module in an image, the same offer `create module`
ends with; `--image`.

### `check`

Reads every manifest and reports every problem at the line that caused it,
with the counts on the last line: images, modules, flavours, and how many
listed modules the base already provides and nothing therefore builds.

### `generate`

Writes, under `generated/`, the Containerfile for each image, the per-module
build scripts, and both renderings of the capability graph. Lists what it
wrote. The directory is cleared first, so an image or a module that is gone
leaves with its files. `generated/` is tracked; `out/` is scratch and ignored.

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

## Inside a build layer

These read the image around them rather than a repository, run where the
binary is mounted into a build, and are never advertised.

### `os-release`

Writes the image identity the build ARGs carry into `/usr/lib/os-release`.

### `fetch <what> <url> <sha256> [target] [extra...]`

Downloads, verifies against the hash, and places it: `file` keeps it, `tree`
unpacks it, `bin` installs one executable, `rpm` installs the package.

### `validate-image`

Every check a built image has to pass.
