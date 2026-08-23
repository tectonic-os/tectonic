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

x86_64 Linux is what is published. Anywhere else, build it.

Both resolve the latest release at run time, so what arrives is what is tagged
rather than what is on `main`. `tect upgrade` is newer than the current tag, so
a copy installed today does not have it yet.

## What a repository looks like

    repo.kdl              the repository: schema version, tool pin, default
                          image, workflows
    workstation.image.kdl one image; a root .kdl is an image only if it is
                          named image.kdl or ends .image.kdl
    modules/<path>/       one module apiece, each with a module.kdl

`tect create repo` writes that tree, along with the build scripts, the shell
helpers, the disk config and the workflows, which the tool ships and a
repository does not carry.

See [the schema](docs/schema.md), which documents
[the repository file](docs/schema.md#the-repository-file),
[image files](docs/schema.md#image-files) and
[module manifests](docs/schema.md#module-manifests). Its reference tables are
generated from the tables the parser reads, so they cannot drift from what the
tool accepts.

## Commands

This is the whole surface today. The rest of it is not built yet.

    tect upgrade          replace this tect and its assets with the latest
    tect create repo [name]
                          start a repository, and offer an image in it
    tect create image [name]
                          add an image: what it is called, and what it builds
                          on
    tect create module [name]
                          write a module, with the packages it installs, and
                          offer to list it in an image
    tect import module [name]
                          reference a module from a collection repo.kdl names
    tect copy module [name]
                          copy a collection module into modules/<name>
    tect create key <kind>
                          generate a key one of the repository's modules
                          declares, with both halves under keys/
    tect check            validate every manifest, and say where and why
    tect generate         write the Containerfile per image, the per-module
                          build scripts and the graph, under generated/
    tect build [target]   verify the build files, then build the image
    tect section [image]  the generated Containerfile module section
    tect graph [--format md|json]
                          the capability graph, as markdown holding a mermaid
                          diagram, or as JSON
    tect why [module]     one module's trust read-out: what builds it, what it
                          exchanges, what it claims, and where it came from

`tect plan --json`, `verify`, `summary`, `sbom`, `fetch modules` and
`registry` are the build's, and `os-release`, `build-record`, `fetch` and
`validate-image` run only inside a build layer. See
[the commands](docs/commands.md), which documents all of them, and how every
command takes a flag for everything it needs.

Data goes to stdout and diagnostics to stderr. Exit 1 is the invocation, exit
2 the repository.

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
compared byte for byte against a committed file, and so is the generated half
of `docs/schema.md`. `UPDATE_GOLDEN=1 cargo test` regenerates them, and the
diff is the review.

## Licence

Apache 2.0. See [LICENSE](LICENSE).
