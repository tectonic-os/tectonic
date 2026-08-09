# tect

`tect` is the build tool for a bootc image repository. The repository declares
what its images are made of, in KDL, and `tect` resolves that into everything
the build needs: the order the modules layer in, the options each one is given,
the capabilities they satisfy for each other, and the generated Containerfile
section that comes out of it.

Nothing in the build derives a name, a target, a tag or an order. The tool
does, once, and the shell that runs inside a layer is handed the result.

## What a repository looks like

    repo.kdl              the repository: schema version, default image, workflows
    workstation.kdl       one image, and every root .kdl but repo.kdl is one
    modules/<path>/       one module apiece, each with a module.kdl

`tect init` writes that tree, along with the build scripts, the build phases,
the disk config and the workflows, which the tool ships and a repository does
not carry.

See [the schema](docs/schema.md), which documents
[the repository file](docs/schema.md#the-repository-file),
[image files](docs/schema.md#image-files) and
[module manifests](docs/schema.md#module-manifests). Its reference tables are
generated from the tables the parser reads, so they cannot drift from what the
tool accepts.

## Commands

This is the whole surface today. The rest of it is not built yet.

    tect init [name]      write a new repository into --root, else a directory
                          named for the image, else here
    tect module import <name>
                          copy a module out of a collection repo.kdl names,
                          into modules/<owner>/<name>
    tect check            validate every manifest, and say where and why
    tect plan [--json]    every fact the repository derives, as one document
    tect section [image]  the generated Containerfile module section
    tect graph [--format md|json]
                          the capability graph, as markdown holding a mermaid
                          diagram, or as JSON
    tect generate         write the Containerfile per image, the per-module
                          build scripts and the graph, under generated/
    tect verify           re-emit all of that and compare it against what is
                          committed under generated/

Inside a build layer, where the binary is mounted and there is no repository
to read:

    tect os-release       write the image identity into /usr/lib/os-release
    tect fetch ...        download, verify against a hash, and place it
    tect validate-image   every check a built image has to pass

Data goes to stdout and diagnostics to stderr. Exit 1 is the invocation, exit
2 the repository.

## Building it

    cargo build --release

There is no tagged release yet, so a repository cannot fetch a binary. Until
there is one, run the local build against a repository with `--root`, and
point `TECT_ASSETS` at this repository's `assets/` so `tect init` can find the
scaffolding it copies.

## Developing

    ./lint.sh             shellcheck, shfmt, rustfmt and the tests, as CI runs it
    ./lint.sh --fix       rewrite everything into the format it gates on

The tests are goldens: every command, over this repository's fixtures, is
compared byte for byte against a committed file, and so is the generated half
of `docs/schema.md`. `UPDATE_GOLDEN=1 cargo test` regenerates them, and the
diff is the review.

## Licence

Apache 2.0. See [LICENSE](LICENSE).
