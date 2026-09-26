# tect

`tect` is the build tool for a bootc image repository. The repository declares
what its images are made of, in KDL, and `tect` resolves that into everything
the build needs: the order the modules layer in, the options each one is given,
the capabilities they satisfy for each other, and the generated Containerfile
section that comes out of it.

Nothing in the build derives a name, a target, a tag or an order. The tool
does, once, and the shell that runs inside a layer is handed the result.

## Installing it

    curl -fsSL https://raw.githubusercontent.com/tectonic-os/tectonic/main/install.sh | sh

That places the binary and the scaffolding it copies, together:
`~/.local/bin/tect` with `~/.local/share/tectonic/assets`, or both under
`/usr/local` when it is run as root. A `tect` that arrives without a matching
`assets/` scaffolds from whatever stale copy the host already has, silently, so
the two halves are never moved apart. `tect upgrade` does the same afterwards
without needing the URL again: it says what is running and what the latest
release is, moves both halves, and says so instead when there is nothing to
move.

x86_64 and aarch64 Linux are what is published. Anywhere else, build it.

Both resolve the latest release at run time, so what arrives is what is tagged
rather than what is on `main`.

## What a repository looks like

    repo.kdl              the repository: schema version, tool pin, default
                          image, workflows
    workstation.image.kdl one image; a root .kdl is an image only if it is
                          named image.kdl or ends .image.kdl
    modules/<path>/       one module apiece, each with a module.kdl

`tect create repo` writes that tree along with the disk config, the root
dotfiles and the Containerfile skeleton it scaffolds. `tect generate`, the step
it prints next, writes the build scripts, the shell helpers and the workflows,
which the tool ships and a repository does not carry.

See [the schema](docs/schema.md), which indexes
[the repository file](docs/schema/repo.md),
[image files](docs/schema/image.md),
[module manifests](docs/schema/module.md),
[the base catalog](docs/schema/bases.md) and
[pins](docs/schema/pins.md). Its reference tables are
generated from the tables the parser reads, so they cannot drift from what the
tool accepts.

## Commands

`tect` with nothing after it opens a picker of what runs here where the output
is a terminal, and prints the list where it is not. `tect --help` keeps every
command a person runs and groups them by where they run. A verb with no noun,
such as `tect create` or `tect vm`, opens a picker of its nouns. Leaving a
picker is not an error: it exits 0 having done nothing.

[The commands](docs/commands.md) documents every command with its flags. It is
generated from the definition the parser reads, and `tect <command> --help`
prints the same prose. The reference lists the global flags once, under `tect`.

The help a person reads names the commands that run anywhere or in a
repository. The build's own commands are the `Script` family — `plan`,
`verify`, `summary`, `sbom`, `fetch modules`, `scap` and its nouns, `registry`
and its nouns, and `recipe` — which is the contract the build runs against.
The `Layer` family — `os-release`, `build-record`, `fetch` and
`validate-image` — reads the image around them, and runs only where the binary
is mounted into a build layer.

The repository is the nearest directory at or above the working directory
holding a `repo.kdl`, or `--root <dir>`. Data goes to stdout and diagnostics to
stderr. Exit 1 is the invocation, exit 2 the repository.

### Flags and prompts

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

`create repo`, `create image`, `create flavour`, `create module`, `import
module`, `copy module` and `generate` end with a tree of the files they wrote,
rooted at the repository, and a leaf a later step took further carries a phrase
saying what it added. `create key` names its two halves instead: one of them is
private and ignored, so a tracked-file tree omits it.

`--root` and `--no-tui` are accepted by every command. A flag a command does
not take is refused rather than ignored, including the switches: `--cache-to`
and `--no-cache-from` belong to `build`, and `--rebuild` to the three `vm`
nouns.

### On a booted image

A built image carries `/usr/share/tectonic/manifest.json`, what it declares it
is made of, and `/usr/share/tectonic/build.json`, what the build resolved. With
no `repo.kdl` anywhere above, `why`, `summary`, `scap content` and `plan`
answer off those two and need no checkout. A repository wins whenever there is
one, because it is the more specific answer and it has the source.

Everything else needs the source tree and says so, naming what does answer here.

**Every host answer is scoped to the target the record says this image was
built as.** The manifest holds every target the repository declares, so an
unscoped answer would describe an image that is not this one; a `summary` or
`scap content` naming a different target is refused. If the record names no
target, a one-target manifest is that target; with more than one, `why` reads
across all of them and says so, and `summary` and `scap content` are refused
because no honest answer exists.

## Building it

    cargo build --release

A local build is not an install, and running one is where the assets bite:
`tect` looks for an `assets` directory beside the binary and then at the
installed paths, so a binary out of `target/` scaffolds from whatever copy the
host has, silently. Point `TECT_ASSETS` at this repository's `assets/` when you
run one, and aim it at a repository elsewhere with `--root`.

## Developing

    ./lint.sh             shellcheck, shfmt, rustfmt and the tests, as CI runs it
    ./lint.sh --fix       rewrite everything into the format it gates on

The tests are goldens: every command, over this repository's fixtures, is
compared byte for byte against a committed file, and so are
`docs/commands.md` and the generated half of `docs/schema.md` and
`docs/schema/`.
`UPDATE_GOLDEN=1 cargo test` regenerates them, and the diff is the review.

## Licence

Apache 2.0. See [LICENSE](LICENSE).
