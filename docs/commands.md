# Commands

`tect --help` lists the commands a person runs: `upgrade` and `create repo`,
and the ones that need a repository. The two families at the end of this file — what a build
runs against, and what runs inside a build layer — are the contract rather than
the help, and are not in that list.

`tect` with nothing after it opens a picker where the output is a terminal, and
prints the list where it is not. A verb with no noun — `tect create`, `tect
copy`, `tect import`, `tect registry`, `tect set`, `tect vm` — opens a picker of
its nouns. **The picker offers what runs where it was typed**, with each row's own
description; the help above keeps every row and groups them by where they run.
Leaving a picker is not an error: it exits 0 having done nothing.

The repository is the nearest directory at or above the working directory
holding a `repo.kdl`, or `--root <dir>`. Data goes to stdout, diagnostics to
stderr. Exit 1 is the invocation, exit 2 the repository.

## On a booted image

A built image carries `/usr/share/tectonic/manifest.json` — what it declares it
is made of — and `/usr/share/tectonic/build.json`, what the build resolved. With
no `repo.kdl` anywhere above, `why`, `summary`, `scap content` and `plan`
answer off those two and need no checkout. A repository wins whenever there is
one, because it is the more specific answer and it has the source.

Everything else needs the source tree and says so, naming what does answer here.

**Every host answer is scoped to the target the record says this image was
built as.** The manifest holds every target the repository declares, so an
unscoped answer would describe an image that is not this one; a `summary` or
`scap content` naming a different target is refused. A record that names no
target falls back to reading across all of them and says in the output that it
did.

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

`create repo`, `create image`, `create flavour`, `create module`, `import
module`, `copy module` and `generate` end with a tree of the files they wrote,
rooted at the repository, each leaf carrying a phrase saying what it is for. `create key` names its two halves
instead: one of them is private and ignored, so a tracked-file tree omits it.

`--root <dir>` is accepted by every command below. A flag a command does not
read is an error, not a silent no-op.

## Upgrading the tool

### `upgrade`

Replaces this `tect`, and the assets it scaffolds from, with the latest
published release. It takes no argument and no flag.

It prints the running version and the latest one, and stops there when they are
the same or when the running build is ahead of the tag.

#### Notes:
- The binary and the assets move together. A binary that arrived alone would
  leave the host scaffolding from whatever stale `assets/` it already had, with
  no diagnostic, which is the failure this command exists to prevent.
- Where the two go is chosen by who runs it, and never falls back between the
  pairs: root takes `/usr/local/bin` with `/usr/local/share/tectonic/assets`,
  anyone else takes `~/.local/bin` with `$XDG_DATA_HOME/tectonic/assets`, which
  is `~/.local/share/tectonic/assets` unless that variable says otherwise. A
  destination it cannot write is refused, naming the other pair, because
  falling back to one you did not ask for is the guess it exists not to make.
- The assets are swapped rather than merged, so a file a release dropped does
  not survive into every repository created afterwards.
- The download is verified against the `.sha256` published beside it before
  anything on disk is touched, so a refusal leaves the existing install intact.
- An `assets` directory beside the binary outranks the pair this places, and is
  refused before anything is fetched. A set `TECT_ASSETS` outranks it too, but
  that is environment rather than disk and may be deliberate, so it warns.
- x86_64 Linux is what is published. Anywhere else it refuses by name rather
  than fetching a 404.
- On a machine with no `tect` yet, the same thing is
  `curl -fsSL https://raw.githubusercontent.com/tectonic-os/tectonic/main/install.sh | sh`.

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
  branch head, so every fetch takes whatever the branch holds then and no
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
- The `modules` block opens with whatever fills the family-adapter role for the
  base's family: the module that `supports` it and `provides
  "build-environment"`, taken from the collections `sources` declares. It is an
  ordinary line, so deleting it is how you decline it. A collection this
  machine has not fetched seeds nothing, and the reference the seed writes
  resolves on the next `tect fetch modules`.

### `create flavour [name]`

Writes one flavour into an image's `flavours` block, creating the block when
the image has none. A flavour is published as `<image>-<flavour>` beside the
image's ungated build.

Asks you for:
- The flavour name
- Which image publishes it, when `--image` names none

#### Flags:
    --image <name>    the image that publishes the flavour

#### Notes:
- Neither `default` nor `pr-build` is written. Both are edits: `default`
  changes what a bare `--target <image>` builds, which is not a thing to write
  into a file on your behalf.
- Listing a module under the flavour is a separate step; the `flavours` block
  declares the flavour, the `modules` block gates the modules.
- A name the image already declares is refused, as is `none`, which is what the
  ungated build is called.

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
    --image <name>        list the module in this image or flavour; repeatable

#### Notes:
- Packages are scaffolded under the repository's own family, as
  `packages { fedora "..." }`, `debian` or `ubuntu`. The generated build layer
  runs `dnf5 install -y` on Fedora and `apt-get update` / `install -y` /
  `clean` on Debian and Ubuntu, per matching group.
- `enablerepo=` is Fedora-only, and naming it on a Debian or Ubuntu group is a
  `check` diagnostic.
- Anything `--with` writes is held to the schema like the rest of the manifest.
- You are asked which of the declared images list the module, with each image's
  flavours under it, and several are an answer: enter or space toggles one on a
  terminal, and `1 3` or `1,3` answers the numbered list where there is not one.
  A repository with one image and no flavours is asked as a yes or a no instead.
- An image and one of its own flavours cannot both be chosen: the ungated entry
  is already in every flavour, so the gated one would be a duplicate. Two
  flavours of one image can, and are two lines in the one file.
- `--image` takes what `--target` takes: `example` for the ungated entry and
  `example/dx` for a flavour of it. A name the repository does not declare is
  refused before the module is written.
- None is an answer too: having a module and listing it in an image are
  different decisions. Leaving the picker instead writes nothing at all.

### `import module [name]`

References modules from a source collection by adding them to an image's
`source` block. The module stays out of the tracked tree; import populates its
ignored `modules/.remote/<owner>/<name>` cache, and a build fetches it again
when the collection pin changes.

Asks you for:
- Which modules, listing every one the collections hold with its description and
  what it requires, when no name is given. Several may be chosen, and they share
  the one listing answer and one of each offer below
- Which images or flavours the modules are listed in; an import with none has no
  repository representation and is refused
- Whether to bring what they require and nothing in those images provides
- Whether to generate the CI they make runnable
- Which profile to be measured against, where they claim benchmark rules a
  profile selects and an image listing them declares no `conforms`

#### Flags:
    --image <name>    list the module in this image or flavour; repeatable
    --datastream <f>  the SCAP content the profile offer is read out of;
                      defaults to the family's installed copy, and no content
                      is no offer

#### Notes:
- The argument names one module. The picker is what takes several, since one
  answer per question is what makes a set cheaper than a module at a time.
- A bare name is searched for in every collection. `<owner>/<name>` picks
  between two collections that both have it.
- A pinned collection member is downloaded and verified once. An `unpinned`
  member is downloaded unverified each time, and audit enforcement refuses the
  reference.
- An image already listing the module is refused here rather than at the next
  command that reads the file. A module gated to two flavours is listed under
  each, so only an overlap is a duplicate.
- The requirements the offer brings in are listed in the same images, ahead of
  the module itself. Declining leaves a file that is still valid; `check` then
  names the import that would satisfy what is missing.
- Nothing here is automatic. Both offers default to no where there is nobody to
  ask, so a scripted import writes exactly what it was told to.

### `copy module [name]`

Copies modules out of a source collection into `modules/<name>` and records
each one's source in `provenance.kdl`. The repository owns them from then on:
nothing fetches over them and no pin moves them.

It is `import module` with a different ending — the same picker, the same
questions, the same offers, the same flags and the same refusals — and differs
only in these three ways:
- The modules land under `modules/` and are listed by their own names, with no
  `source` block, so what the requires offer brings is vendored too
- No image is not a refusal. A copied module is in the repository whether one
  lists it or not, and the listing offer is the second half of the job rather
  than the whole of it
- Something already at `modules/<name>` is refused, and the refusal names the
  collection the thing there came from

### `create key <kind>`

Generates one of the keys the repository's modules declare, and writes:
- The public half, under `keys/public/` at the path it has in the image
- The private half, under `keys/private/` with the name the declaration gives it

Asks you for:
- Which kind, listing the kinds the modules declare, when no argument is given

#### Flags:
    --module <name>   which module, where two of them declare the same kind
    --cn <name>       the certificate common name; the repository directory name by default

#### Notes:
- `keys/private/` is covered by the scaffolded `.gitignore`. A repository
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

### `set conforms [image]`

Chooses the benchmark profile a scan measures one image against, and writes it
into that image's `conforms`. Naming no image picks the only one, or asks which.

Asks you for:
- Which profile, out of the ones the datastream carries, with what each is
  called beside it
- Whether to import the collection modules claiming rules that profile selects
  and nothing the image lists claims

#### Notes:
- `--datastream <file>` names the content to choose out of. Without it this
  reads the copy installed on this machine for the image's family, and refuses
  by naming `scap-security-guide` and the flag when there is none. `tect
  coverage` never probes the host; this does, because a profile written into an
  image has to be one the scan that measures it will carry.
- The question says the cost first: a `conforms` is the whole scan gate, so
  declaring one turns the image scan on for every build. Where the repository
  declares `audit { enforce #true }` it says that a rule the image fails fails
  the build instead.
- It is not a claim to pass. `conforms` is what the image is measured against,
  and declaring one before reaching it is the point.
- A second run replaces the declaration rather than adding a second one.
- The import offer covers collection members only. A module the repository owns
  needs a line rather than an import, which is what `check` already says.
- There are no toggle flags, the same way `set workflows` has none: with nobody
  to ask this says to write the line into the image file.

### `set claims <module>`

Chooses the benchmark rules one module this repository holds claims to cover,
and writes them into its `satisfies` block as numbers.

Asks you for:
- Which profile the rules are read out of, out of the ones the datastream
  carries
- Which of that profile's rules the module claims, as a tree grouped by the
  numbers' own dotted sections, opening on what it already claims

#### Notes:
- `--datastream <file>` names the content, and without it this reads the copy
  installed on this machine for the family the module `supports`, refusing by
  naming `scap-security-guide` and the flag when there is none. The contract is
  `set conforms`'s: a number written into a manifest has to be one the scan that
  measures it will carry.
- Nothing here reads a scan. A claim is what the module says it supplies, and
  `tect scap` is what measures whether the built image kept it.
- The row carries the number, the rule's title and the rule the number resolves
  to, so a number naming more than one rule is visible while it is being chosen.
  A rule no number of its own reaches is left out and counted in a line above
  the question, since no `satisfies` could name it.
- Choosing a group chooses every rule under it, and the numbers are written out
  under one benchmark node, a number per line. No prefix ever reaches the file:
  a claim that grows when the content does is not a claim.
- That node is named for the profile the rules were chosen out of, and for
  nothing else. The family is already declared in `supports`, and a name that
  folds one in is a second spelling that can disagree with the first.
- A claim about a rule the chosen profile does not select is kept, so measuring
  a module against a second profile does not drop what the first one wrote.
- Claiming nothing takes the block away, the way `set workflows` takes the
  workflow block away. Leaving either picker changes nothing.
- The benchmark each number is written under is `<profile>-<family>`, and it is
  decorative: a number resolves against the datastream, never against the name
  it was written under.

### `check`

Reads every manifest and reports every problem at the line that caused it, then
the counts on the last line: images, modules, flavours, and how many listed
modules the base already provides.

#### Flags:
    --datastream <f>      the SSG content, for the conformance read-out

#### Notes:
- Above the counts it names every base a collection describes differently from
  the tool's own entry, every collection declared `unpinned`, every image
  declaring a `conforms` nothing it lists claims a rule of, and every
  `module.kdl` sitting below another member's directory in a collection.
- A member inside a member is invisible everywhere else: the walk stops at the
  first `module.kdl`, and everything below one is that module's own content. It
  is left that way — descending would make a member's own subdirectory
  ambiguous — so what `check` fixes is the silence, not the walk.
- None of those is an error, and none changes the exit code. An image is
  allowed to declare a target it has not reached; that is what declaring one
  first is for.
- The conformance read-out has two tiers. Without `--datastream` it reads the
  manifests alone, so it can only report an image measured against a profile
  that lists no module declaring `satisfies`. With one it says how many of the
  profile's rules nothing listed claims and which modules would claim them,
  and it names any declared collection nothing read rather than concluding
  from its silence. `tect scap content` prints the path to pass it.

### `generate`

Writes the build files and draws the tree of what it wrote:
- `generated/<image>/Containerfile`, one per image
- `generated/<image>/modules/<module>.sh`, the per-module build scripts
- `generated/<image>/finalize.sh`
- `generated/<image>/graph.md` and `graph.json`, both renderings of the
  capability graph
- `generated/plan.json`
- `generated/seed.kdl`, where `repo.kdl` nominates a seedable image
- Every workflow the `workflows` block names, under `.github/workflows/`

#### Notes:
- Everything one image generates lives under a directory named for it, so what
  belongs to the repository and what belongs to one image are not the same
  pile. The Containerfile is called `Containerfile` rather than the image's
  name, which left the one file a person goes looking for with no extension to
  find it by.
- Referenced modules are fetched first, as `build` does. What is written is read
  off `modules/.remote/`, so a tree older than the collection would bake a
  deleted file into `plan.json` and CI — which fetches into an empty tree —
  would disagree and be right.
- `generated/` is cleared first, so an image or module that is gone leaves with
  its files. A workflow is removed by name instead: one the tool does not ship
  is the repository's own and is left where it is.
- A terminal gets the tree; a pipe or a redirect gets the flat list of paths.
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

### `vm build|run|spawn <type>`

Turns the container image into a disk and boots it. `<type>` is `qcow2`, `raw`
or `iso`, asked for where there is a terminal to ask on. `build` converts,
`run` boots under qemu and converts first where the disk is missing, and
`spawn` boots it with systemd-vmspawn, which cannot boot an installer iso.

#### Flags:
    --target <t>          what a rebuild builds, and what an iso installs
    --image <ref>         the container image to convert, without its tag
    --tag <tag>           its tag, else $DEFAULT_TAG, else latest
    --ram <size>          memory for the virtual machine
    --rebuild             build the container image first

#### Notes:
- This is `scripts/vm.sh`, which `generate` writes and this execs. The terminal
  is the script's: it asks for sudo and boots a machine onto it, and nothing
  here captures or reimplements any of that. Every default is the script's too,
  so a flag not passed is not restated in Rust.
- The disk is the size `disk_config/disk.toml` declares. Its
  `[[customizations.filesystem]]` root is applied by the image builder, and the
  disk comes out that size plus about 1.5 GiB of ESP and `/boot`.
- It reads the image out of rootful podman, so it asks for sudo and copies the
  image into root's store with `podman image scp` where it is not there.
- Building a disk is fedora-only: the image builder cannot build one from an
  image carrying no SELinux policy.

### `section [image]`

Prints the generated Containerfile module section for one image, the default
image when none is named.

### `graph`

Prints the default image's capability graph: what provides what, what requires
it, what only orders against it, and what the base already carries.

#### Flags:
    --format <md|json>    markdown holding a mermaid diagram by default, or json

### `coverage [image] [--format md|json]`

Every rule the profile an image declares `conforms` to selects, the number a
claim names it by, which of the image's modules claims it, and which module in
the repository or its collections would claim one nothing does. The default
image when none is named, a picker where there is a terminal to pick on.

#### Flags:
    --datastream <f>      the SSG content the profile is read out of
    --format <md|json>    markdown, the default, or json

#### Notes:
- No scan is involved and no report is read. This says what is claimed, not
  what passes; `tect scap` is what measures.
- The content is only ever the one `--datastream` names. Nothing probes the
  host for installed SSG, so what this prints does not depend on the machine.
  `tect scap content` prints the path a scan of this repository would use.
- A terminal draws it as a table, red for a rule nothing claims. Everything
  else gets the markdown, so `tect coverage > report.md` is the export.
- The counts and the collections nothing read go to stderr, since the second
  is about this machine rather than about the image.
- A rule with no number in the `Number` column is one no `satisfies` can name.
  It is unclaimable rather than unclaimed.

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

Fetches every out-of-tree module the images reference, verifies one whose pin
has a hash, and puts it under `modules/.remote/`.

#### Notes:
- A tree already at its pin is left alone; one no image references any more is
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
    --base-scan <f>       what the bare base passed alone, read only

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
- `--base-scan` names a pass set a scan of the bare base wrote, in the same
  format `--baseline` writes, and adds a *base alone* column. A claim the base
  already passes is reported not load-bearing, which is a notice and never a
  finding: the module may implement the rule as well, and it applies its
  settings either way. The document records passes only, so a rule missing from
  it is not a rule the base failed.
- Findings are fatal only under `audit { enforce #true }`, like every other
  audit fact, and the report goes to stdout either way.

### `scap content`

Prints the datastream the target is measured with, and nothing at all when the
image declares no `conforms`, which is an image asking not to be scanned. This
is what the scan job gates on, and `tect set conforms` is what opens it.

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

### `why [module] [--format md|json]`

One module's trust read-out: which targets build it, what it provides and who
requires that, what it requires and what provides it, what it claims to
harden, and where every byte of it came from — the collection it was imported
from and at what pin, whether it has been edited since, what it fetches, and
whether it enables a third-party package repository.

#### Flags:
    --format <md|json>    markdown, the default, or JSON

#### Notes:
- It answers two ways from one renderer. In a repository it reads the resolved
  plan; on a booted image it reads the two baked documents, scoped to the
  target the record names.
- On a host, a baked document written against a schema version this binary does
  not read is refused rather than answered off. The binary in an image is pinned
  independently of the one that built it, so the two can be a schema apart, and
  a host is the one place with no repository to check an answer against. The
  refusal names both numbers and the tool version; `tect plan --json` prints the
  manifest as it stands and is unaffected, since it reads no field out of it.
- On a booted image it also prints the repository the image was built from and
  the commit it was at, with the `git clone` that reaches them. The module tree
  is deliberately not in the finished image, so comparing this machine against
  its declarations means fetching them rather than reconstructing them.
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
