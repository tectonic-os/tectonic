# Commands

This document contains the help content for the `tect` command-line program.

**Command Overview:**

* [`tect`↴](#tect)
* [`tect upgrade`↴](#tect-upgrade)
* [`tect create`↴](#tect-create)
* [`tect create repo`↴](#tect-create-repo)
* [`tect create image`↴](#tect-create-image)
* [`tect create flavour`↴](#tect-create-flavour)
* [`tect create module`↴](#tect-create-module)
* [`tect create key`↴](#tect-create-key)
* [`tect import`↴](#tect-import)
* [`tect import module`↴](#tect-import-module)
* [`tect copy`↴](#tect-copy)
* [`tect copy module`↴](#tect-copy-module)
* [`tect set`↴](#tect-set)
* [`tect set workflows`↴](#tect-set-workflows)
* [`tect set conforms`↴](#tect-set-conforms)
* [`tect set claims`↴](#tect-set-claims)
* [`tect set key`↴](#tect-set-key)
* [`tect check`↴](#tect-check)
* [`tect generate`↴](#tect-generate)
* [`tect build`↴](#tect-build)
* [`tect vm`↴](#tect-vm)
* [`tect vm build`↴](#tect-vm-build)
* [`tect vm run`↴](#tect-vm-run)
* [`tect vm spawn`↴](#tect-vm-spawn)
* [`tect section`↴](#tect-section)
* [`tect graph`↴](#tect-graph)
* [`tect why`↴](#tect-why)
* [`tect coverage`↴](#tect-coverage)
* [`tect plan`↴](#tect-plan)
* [`tect verify`↴](#tect-verify)
* [`tect summary`↴](#tect-summary)
* [`tect sbom`↴](#tect-sbom)
* [`tect scap`↴](#tect-scap)
* [`tect scap content`↴](#tect-scap-content)
* [`tect scap tailoring`↴](#tect-scap-tailoring)
* [`tect scap rules`↴](#tect-scap-rules)
* [`tect fetch`↴](#tect-fetch)
* [`tect fetch modules`↴](#tect-fetch-modules)
* [`tect registry`↴](#tect-registry)
* [`tect registry namespace`↴](#tect-registry-namespace)
* [`tect registry ref`↴](#tect-registry-ref)
* [`tect recipe`↴](#tect-recipe)
* [`tect os-release`↴](#tect-os-release)
* [`tect build-record`↴](#tect-build-record)
* [`tect validate-image`↴](#tect-validate-image)

## `tect`

the build tool for a bootc image repository

**Usage:** `tect [OPTIONS] [COMMAND]`

###### **Subcommands:**

* `upgrade` — replace this tect and its assets with the latest
* `create` — start a repository, or add an image, flavour, module or key to one
* `import` — reference a collection module from an image
* `copy` — copy a collection module into this repository
* `set` — choose what this repository declares, or record a key
* `check` — read every manifest and say what is wrong with it
* `generate` — write the build files, and list what was written
* `build` — verify the build files, then build the image
* `vm` — turn the built image into a disk, and boot it
* `section` — print the Containerfile section an image generates
* `graph` — print what provides what, and what the base carries
* `why` — print one module's trust read-out, byte by byte
* `coverage` — print who claims each rule the image conforms to
* `plan` — print every fact this repository derives, as json
* `verify` — byte-compare what is generated against what is committed
* `summary` — print what one target is made of, as a markdown table
* `sbom` — print the pinned payloads one target carries, as SPDX
* `scap` — print what one scan says about the target
* `fetch` — download one payload, verify it, and place it
* `registry` — print where images publish, and under what reference
* `recipe` — print the installer recipe one target installs from
* `os-release` — write the image identity the build ARGs carry
* `build-record` — write the record of what the build resolved
* `validate-image` — run every check a built image has to pass

###### **Options:**

* `--root <dir>` — the repository, else the nearest `repo.kdl` at or above the working directory
* `--no-tui` — ask nothing, and fail naming the flag a missing answer needs



## `tect upgrade`

Replaces this `tect`, and the assets it scaffolds from, with the latest
published release. It takes no argument and no flag.

It prints the running version and the latest one, and stops there when they are
the same or when the running build is ahead of the tag.

**Usage:** `tect upgrade`

Notes:

- The binary and the assets move together. A binary that arrived alone would
  leave the host scaffolding from whatever stale `assets/` it already had, with
  no diagnostic, which is the failure this command exists to prevent.
- Where the two go is chosen by who runs it, and never falls back between the
  pairs: root takes `/usr/local/bin` with `/usr/local/share/tectonic/assets`,
  anyone else takes `~/.local/bin` with `$XDG_DATA_HOME/tectonic/assets`, which
  is `~/.local/share/tectonic/assets` unless that variable says otherwise. A
  destination it cannot write is refused, naming the other pair, because
  falling back to a pair the user did not ask for is the guess it exists not to
  make.
- The assets are swapped rather than merged, so a file a release dropped does
  not survive into every repository created afterwards.
- The download is verified against the `.sha256` published beside it before
  anything on disk is touched, so a refusal leaves the existing install intact.
- An `assets` directory beside the binary outranks the pair this places, and is
  refused before the release is fetched. A set `TECT_ASSETS` outranks it too, but
  that is environment rather than disk and may be deliberate, so it warns.
- x86_64 and aarch64 Linux are what is published. Anywhere else it refuses by
  name rather than fetching a 404.
- On a machine with no `tect` yet, the same thing is
  `curl -fsSL https://raw.githubusercontent.com/tectonic-os/tectonic/main/install.sh | sh`.



## `tect create`

start a repository, or add an image, flavour, module or key to one

**Usage:** `tect create [COMMAND]`

###### **Subcommands:**

* `repo` — start a repository for your own images
* `image` — add an image: its name, and what it builds on
* `flavour` — add a gated module set an image also publishes
* `module` — write a module, and offer to list it in an image
* `key` — generate a key one of this repository's modules declares



## `tect create repo`

Writes a new repository with the following into the current directory unless a `--root`
location is defined in the command:

- A `repo.kdl` file
- A module directory
- A `scripts/` directory holding the Containerfile skeleton
- A `disk_config/` directory

**Usage:** `tect create repo [OPTIONS] [name]`

Notes:

- `--host` and `--owner` compose into the origin every image URL is built from.
  It asks for them only when the images build on a schedule.
- On github it offers to create the repository, and says what to do where `gh`
  is missing or logged out.
- `git init` is all of git the tool does. The first commit, the remote and the
  push are the user's, and the closing lines are the commands for them.
- The `repo.kdl` it writes declares `tectonic-os/modules` in `sources`, so
  `import module` works immediately. That collection is `unpinned`: it follows a
  branch head, so every fetch takes whatever the branch holds then and no
  `sha256` checks what arrived. Delete the block or replace it with a tagged and
  hashed pin if that trade is not the trade the user wants.
- A repository does not nest, and one inside another is refused.

###### **Arguments:**

* `<name>`

###### **Options:**

* `--host <domain>` — where the repository is hosted; github.com by default
* `--owner <name>` — your account or org on the repository host
* `--image <name>` — write a first image too; `create image` adds one later
* `--base <ref>` — the bootc image the first image is based on



## `tect create image`

Writes one image file, `<image-id>.image.kdl`, at the repository root. The id
derives from the name, so "My Desktop" writes `my-desktop.image.kdl`. A root
`.kdl` is an image only when it is named `image.kdl` or ends in
`.image.kdl`; anything else is reported rather than read.

Prompts for:

- The image name, which defaults to what the repository is called
- The base it builds on, picked from the catalog or given as any bootc reference

**Usage:** `tect create image [OPTIONS] [name]`

Notes:

- A base from the catalog writes its family and what it already ships into
  `base`. A base from outside the catalog prompts for the family and writes no
  `provides`, so `check` reports what no module satisfies.
- `url` and `issues-url` are the repository's, not the image's, since every
  image is published out of one remote. A repository that declared no origin
  writes neither, and a second image matches the first.
- Writing a second image into a repository that declares no `default-image`
  appends one naming the image already there, so a bare build builds what it
  built before. Nothing else in `repo.kdl` moves.
- The `modules` block opens with what the base cannot build without, offered as
  one question naming the modules: whatever fills the family-adapter role for
  the base's family (the module that `supports` it and `provides
  "build-environment"`) and whatever satisfies each capability the base row
  `requires`. Both come out of the collections `sources` declares, both are
  missing on a fresh repository, and both read as one question. Declining
  writes an empty block, and every line it writes is an ordinary one, so
  deleting one afterwards is the same decision.
- Answering that question needs the collections read, so where one is declared
  and not on this machine it is fetched here. Nothing is fetched when the
  collections already on disk answer the question, and nothing is fetched with
  nobody to ask: a `--no-tui` run offers only what is already there. A
  repository that does not exist yet caches that fetch outside itself and
  throws it away, so a run left at the review screen leaves nothing behind.
  That does mean re-editing the base re-fetches.

###### **Arguments:**

* `<name>`

###### **Options:**

* `--owner <name>` — your account or org, where no image already carries one
* `--base <ref>` — the bootc image this image is based on; skips the picker



## `tect create flavour`

Writes one flavour into an image's `flavours` block, creating the block when
the image has none. A flavour is published as `<image>-<flavour>` beside the
image's ungated build.

Prompts for:

- The flavour name
- Which image publishes it, when `--image` names none

**Usage:** `tect create flavour [OPTIONS] [name]`

Notes:

- Neither `default` nor `pr-build` is written. Both are edits: `default`
  changes what a bare `--target <image>` builds, and the tool writes no such
  change on the user's behalf.
- Listing a module under the flavour is a separate step; the `flavours` block
  declares the flavour, the `modules` block gates the modules.
- A name the image already declares is refused, as is `none`, which is what the
  ungated build is called.

###### **Arguments:**

* `<name>`

###### **Options:**

* `--image <name>` — the image that publishes the flavour



## `tect create module`

Writes one module manifest, `modules/<name>/module.kdl`. The name may be a
path, so `create module apps/firefox` writes
`modules/apps/firefox/module.kdl`.

Prompts for:

- Whether the module installs packages, and which ones
- Which images the module is listed in, if any

**Usage:** `tect create module [OPTIONS] [name]`

Notes:

- Packages are scaffolded as one flat `packages` line on every family the
  module supports; a `family` gate is where names that differ go. The generated
  build layer installs through the family's adapter: `dnf install -y` on Fedora
  and RHEL, `apt-get update` / `install -y` / `clean` on Debian and Ubuntu.
- `enablerepo=` is dnf's, and naming it on a Debian or Ubuntu batch is a
  `check` diagnostic. One naming a COPR is Fedora's alone.
- Anything `--with` writes is held to the schema like the rest of the manifest.
- It asks which declared images list the module, with each image's flavours under
  it; several are an answer. Enter or space toggles one on a terminal, and `1 3`
  or `1,3` answers the numbered list where there is not one. A repository with
  one image and no flavours is asked as a yes or a no instead.
- An image and one of its own flavours cannot both be chosen: the ungated entry
  is already in every flavour, so the gated one would be a duplicate. Two
  flavours of one image can, and are two lines in the one file.
- `--image` takes what `--target` takes: `example` for the ungated entry and
  `example/dx` for a flavour of it. A name the repository does not declare is
  refused before the module is written.
- None is an answer too: having a module and listing it in an image are
  different decisions. Leaving the picker instead writes nothing at all.

###### **Arguments:**

* `<name>`

###### **Options:**

* `--image <name>` — list the module in this image or flavour; repeatable
* `--pkg <name>` — a package the module installs; repeatable
* `--with <verb=value>` — one more line in the manifest, such as `--with provides=browser`; repeatable



## `tect create key`

Generates one of the keys the repository's modules declare, and writes:

- The public half, under `keys/public/` at the path it has in the image
- The private half, under `keys/private/` with the name the declaration gives it

Prompts for:

- Which kind, listing the kinds the modules declare, when no argument is given

**Usage:** `tect create key [OPTIONS] [kind]`

Notes:

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
- `openssl profile="pcr-signing"` writes the bare RSA pair a PCR 11 policy is
  signed with, at the declared `bits`: no certificate, so the public half is a
  PEM public key. A sealed UKI embeds it in `.pcrpkey` beside the signature in
  `.pcrsig`, and the machine's first boot verifies them before it writes the
  TPM2 token. Set the private half as the `PCR_PRIVKEY` repository secret; a
  local build reads it through `tect build --secret pcr_privkey=<path>`.
- `openssl profile="tls-ca"` writes a certificate authority: `CA:TRUE` and
  `keyCertSign`, at the declared `bits` and `format`, so a TLS client will take
  the public half as a trust anchor and the private half can issue the server
  certificate it stands behind. It names no repository secret, because the
  issuing happens in a build rather than in CI.
- `ssh-keygen` writes the keypair the user logs in with, ed25519 and with no
  passphrase. `bits` is an RSA size and means nothing here. Nothing in a build
  reads the private half: it is the user's, and the public half is what the
  image ships.

###### **Arguments:**

* `<kind>`

###### **Options:**

* `--module <name>` — which module, where two of them declare the same kind
* `--cn <name>` — the certificate common name; the repository directory name by default



## `tect import`

reference a collection module from an image

**Usage:** `tect import [COMMAND]`

###### **Subcommands:**

* `module` — reference a module from a collection repo.kdl declares



## `tect import module`

References modules from a source collection by adding them to an image's
`source` block. The module stays out of the tracked tree; import populates its
ignored `modules/.remote/<owner>/<name>` cache, and a build fetches it again
when the collection pin changes.

Prompts for:

- Which modules, listing every one the collections hold with its description and
  what it requires, when no name is given. Several may be chosen, and they share
  the one listing answer and one of each offer below
- Which images or flavours the modules are listed in; an import with none has no
  repository representation and is refused
- Whether to bring what they require and nothing in those images provides
- Whether to generate the CI they make runnable
- Which profile to be measured against, where they claim benchmark rules a
  profile selects and an image listing them declares no `conforms`

**Usage:** `tect import module [OPTIONS] [name]`

Notes:

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
- Both offers default to no where there is nobody to ask, so a scripted import
  writes exactly what it was told to.

###### **Arguments:**

* `<name>`

###### **Options:**

* `--image <name>` — list the module in this image or flavour; repeatable
* `--datastream <file>` — the SCAP content the profile offer is read out of; the family's installed copy by default, and no content is no offer



## `tect copy`

copy a collection module into this repository

**Usage:** `tect copy [COMMAND]`

###### **Subcommands:**

* `module` — copy a collection module into this repository



## `tect copy module`

Copies modules out of a source collection into `modules/<name>` and records
each one's source in `provenance.kdl`. The repository owns them from then on:
nothing fetches over them and no pin moves them.

It is `import module` with a different ending: the same picker, the same
questions, the same offers, the same flags and the same refusals. It differs
only in these three ways:

- The modules land under `modules/` and are listed by their own names, with no
  `source` block, so what the requires offer brings is vendored too
- No image is not a refusal. A copied module is in the repository whether one
  lists it or not, and the listing offer is the second half of the job rather
  than the whole of it
- Something already at `modules/<name>` is refused, and the refusal names the
  collection the thing there came from

**Usage:** `tect copy module [OPTIONS] [name]`

###### **Arguments:**

* `<name>`

###### **Options:**

* `--image <name>` — list the module in this image or flavour; repeatable
* `--datastream <file>` — the SCAP content the profile offer is read out of; the family's installed copy by default, and no content is no offer



## `tect set`

choose what this repository declares, or record a key

**Usage:** `tect set [COMMAND]`

###### **Subcommands:**

* `workflows` — choose the CI this repository generates
* `conforms` — choose the benchmark profile an image is measured by
* `claims` — choose the benchmark rules a module claims to cover
* `key` — record a key you already hold, in place of generating one



## `tect set workflows`

Chooses the CI this repository generates, and writes the choice into the
`workflows` block of `repo.kdl`. `tect generate` is what then writes the files.

Prompts for:

- Which of the shipped workflows to generate, opening with the ones already
  declared, and drawing what each of them needs where the repository cannot run
  it
- What time the daily build runs, UTC, where any of the chosen ones has a
  schedule

**Usage:** `tect set workflows`

Notes:

- There are no toggle flags. The declaration file was always the interface, so
  with nobody to ask this says to edit `repo.kdl` rather than naming a flag
  that would be a second way to write the same line.
- Leaving the picker changes nothing. Choosing nothing takes the block away,
  and a repository declaring no block generates no CI.
- `build-disk` needs a fedora or rhel image, because the image builder relabels
  its buildroot with SELinux and builds no disk otherwise. `kernel-freshness`
  needs a module taking a `KERNEL` build arg. One whose basis is absent is drawn
  with the reason and refused.
- Every schedule is an offset from the daily build, so moving one value moves
  all of them. A cron is the one value the emitter writes into a workflow file
  rather than putting in `plan.json`, because the forge reads it out of the
  file before any job exists.



## `tect set conforms`

Chooses the benchmark profile a scan measures one image against, and writes it
into that image's `conforms`. Naming no image picks the only one, or asks which.

Prompts for:

- Which profile, out of the ones the datastream carries, with what each is
  called beside it
- Whether to import the collection modules claiming rules that profile selects
  and nothing the image installs or builds on claims

**Usage:** `tect set conforms [OPTIONS] [image]`

Notes:

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

###### **Arguments:**

* `<image>`

###### **Options:**

* `--datastream <file>` — the SCAP content the profile is chosen out of; the installed copy for the image's family by default



## `tect set claims`

Chooses the benchmark rules one module this repository holds claims to cover,
and writes them into its `satisfies` block as numbers.

Prompts for:

- Which profile the rules are read out of, out of the ones the datastream
  carries
- Which of that profile's rules the module claims, as a tree grouped by the
  numbers' own dotted sections, opening on what it already claims

**Usage:** `tect set claims [OPTIONS] [module]`

Notes:

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
- The benchmark each number is written under is decorative: a number resolves
  against the datastream, never against the name it was written under.

###### **Arguments:**

* `<module>`

###### **Options:**

* `--datastream <file>` — the SCAP content the rules are read out of; the installed copy for the module's family by default



## `tect set key`

Records a public half the user already holds, in place of generating one. Where
`create key` invents a key, this writes down one that exists: a cosign public
key, a MOK certificate and an authorized key are all things the user may
already have, and every kind a module declares can be recorded this way.

Prompts for:

- Which kind, listing the kinds the modules declare, when no argument is given
- The file to read it from, where `--from` did not name one

**Usage:** `tect set key [OPTIONS] [kind]`

Notes:

- The destination is the module's own `public` declaration, so nothing here
  takes a path to write to.
- Only the public half. A private half is not the repository's: a cosign key
  signs in CI, a MOK signs a kernel module, a PCR key signs the machine's TPM
  unlock policy, and an authorized key logs the user in. None of them wants
  its private half copied here.
- The file is read before it is written: a key in the wrong form for the
  generator that would have made it (a PEM public key for `cosign`, a PEM or
  DER certificate for `module-signing`, a bare PEM public key for
  `pcr-signing`, an OpenSSH key line for `ssh-keygen`) is refused here rather
  than by a build a long way from here.
- An existing key is never replaced, exactly as with `create key`.

###### **Arguments:**

* `<kind>`

###### **Options:**

* `--module <name>` — which module, where two of them declare the same kind
* `--from <path>` — the public half to record



## `tect check`

Reads every manifest and reports every problem at the line that caused it, then
the counts on the last line: images, modules, flavours, and how many listed
modules the base already provides.

**Usage:** `tect check [OPTIONS]`

Notes:

- Above the counts it names every base a collection describes differently from
  the tool's own entry, every collection declared `unpinned`, every image
  declaring a `conforms` nothing it installs or builds on claims a rule of, and
  every `module.kdl` sitting below another member's directory in a collection.
- A member inside a member is invisible everywhere else: the walk stops at the
  first `module.kdl`, and everything below one is that module's own content. It
  is left that way, because descending would make a member's own subdirectory
  ambiguous, so what `check` fixes is the silence, not the walk.
- None of those is an error, and none changes the exit code. An image is
  allowed to declare a target it has not reached; that is what declaring one
  first is for.
- The conformance read-out has two tiers. Without `--datastream` it reads the
  manifests alone, so it can only report an image measured against a profile
  that lists no module declaring `satisfies`. With one it says how many of the
  profile's rules nothing listed claims and which modules would claim them,
  and it names any declared collection nothing read rather than concluding
  from its silence. `tect scap content` prints the path to pass it.

###### **Options:**

* `--datastream <file>` — the SSG content, for the conformance read-out



## `tect generate`

Writes the build files and draws the tree of what it wrote:

- `generated/<image>/Containerfile`, one per image
- `generated/<image>/modules/<module>.sh`, the per-module build scripts
- `generated/<image>/finalize.sh`
- `generated/<image>/graph.md` and `graph.json`, both renderings of the
  capability graph
- `generated/plan.json`
- `generated/seed.kdl`, where `repo.kdl` nominates a seedable image
- Every workflow the `workflows` block names, under `.github/workflows/`

**Usage:** `tect generate`

Notes:

- Everything one image generates lives under a directory named for it, so what
  belongs to the repository and what belongs to one image are not the same
  pile. The Containerfile is called `Containerfile` rather than the image's
  name, so the one file the user goes looking for always has a name to find it
  by.
- Referenced modules are fetched first. What is written is read off
  `modules/.remote/`, so a tree older than the collection would bake a deleted
  file into `plan.json` and CI, which fetches into an empty tree, would
  disagree and be right. `build` does not fetch; this is where it happens.
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



## `tect build`

Builds one target: runs `verify` as the drift gate, then execs the container
backend. A target is `<image>/<flavour>`, and the ungated set is named by the
bare image id. The default target when none is named.

**Usage:** `tect build [OPTIONS] [target]`

Notes:

- `$LABELS` adds OCI labels the way `$TAGS` adds tags, and `$IMAGE_VERSION` is
  stamped into the image, defaulting to today in UTC.
- Nothing is fetched and nothing is regenerated here. A build proves the
  committed files are current, and a build that first wrote them would be
  proving nothing. `tect fetch modules` and `tect generate` are what change the
  repository; run them first, or let `tect vm run --rebuild` run all three in
  order.
- A UKI target builds in two passes: the split image, then the whole image
  sealed with that split's storage digest, the number `bootc install`
  compares and a single build cannot read. The digest is computed from the
  local buildah store, so `--backend buildx` is refused for a UKI target, and
  the generated boot chain's tail must carry `FROM ${SPLIT_BASE}` and
  `ARG COMPOSEFS_DIGEST`; a module collection that predates them is refused by
  name. The sealed image is digested from the store again and its UKI cmdline
  read back, both compared with the embedded digest, and only then are the
  requested tags applied, so a failed check leaves nothing published as the
  image that failed it. The skeleton's validation step also runs against the
  sealed image: the seal pass stops at the tail, because a step that runs after
  the seal writes to the image and moves the digest the UKI embedded.

###### **Arguments:**

* `<target>`

###### **Options:**

* `--target <target>` — the target, where the positional argument is not used
* `--tag <tag>` — tag the result; repeatable, and $TAGS adds to it
* `--kernel <name>` — the KERNEL build arg
* `--backend <name>` — buildx or buildah, else $BUILD_BACKEND, else buildah
* `--oci-output <path>` — write an OCI archive instead of loading the image
* `--secret <id=path>` — mount <path> as the build secret <id>; repeatable
* `--cache-to` — export the layer cache to the registry cache repository
* `--no-cache-from` — do not import the layer cache



## `tect vm`

Turns the container image into a disk and boots it. `<type>` is `qcow2`, `raw`
or `iso`, asked for where there is a terminal to ask on. `build` converts,
`run` boots under qemu and converts first where the disk is missing, and
`spawn` boots it with systemd-vmspawn, which cannot boot an installer iso.

**Usage:** `tect vm [COMMAND]`

Notes:

- `--rebuild` is the one form of this that changes the repository, and it runs
  the whole chain in the only order it works in: `fetch modules`, then
  `generate`, then `build`. Without it nothing is fetched, written or built,
  and the disk that is already there is booted.
- This is `scripts/vm.sh`, which `generate` writes and this execs. The terminal
  is the script's: it asks for sudo and boots a machine onto it, and nothing
  here captures or reimplements any of that. Every default is the script's too,
  so a flag not passed is not restated in Rust.
- The disk is the size `disk_config/disk.toml` declares, plus the ESP and `/boot`
  the image builder adds. The `bootc install to-disk` path writes a `DISK_SIZE`
  disk, 20G by default.
- It reads the image out of rootful podman, so it asks for sudo and copies the
  image into root's store with `podman image scp` where it is not there.
- Fedora and RHEL disks use bootc-image-builder. Debian and Ubuntu use `bootc
  install to-disk` with the target's settled `--filesystem` and
  `--composefs-backend` flags for `qcow2` and `raw`. The family is read off the
  target's base, so an `--image` naming a ref this repository does not describe
  is not guessed.
- An `iso` is converted by neither, on any family. It is a live environment
  layered on the target's own image, carrying the installer and the target's
  bytes, and assembled into media by tacklebox, which is built from a source
  pin in the same build. `tect vm build iso` stages the two recipes, the live
  environment's Containerfile and one upstream patch under `out/bootiso/`; they
  are not generated files, because each depends on `--target`, `--tag` and
  `$IMAGE_REGISTRY`, which are build-time rather than commit-time.
- The recipe derives `composeFsBackend`, `genericImage`, the boot chain and
  bootloader, `filesystem`, the admin group and the declared
  `luks-initramfs` witness from the target, and refuses a family it has no
  measured answer for rather than guessing: a wrong value there is a disk that
  is erased and then does not boot. A successful `tect build` inspects the
  initramfs before that witness can enable root encryption. `tect recipe` prints
  the same document.
- Building an `iso` needs a published reference, and `tect vm build iso` says
  so when there is none. The media installs local bytes while recording
  `$IMAGE_REGISTRY`'s reference as the installed machine's update origin, so a
  local namespace would write `localhost/...` into a machine. A disk records
  nothing and is not refused for this.
- `run iso` keeps its target disk under `out/bootiso/storage/`, so completing
  an install and rebooting tests what was installed.
- The live environment autologins root on the console and asks for no VM
  password, because the installer partitions disks and creates the installed
  machine's account itself. That console is the installer media's, never the
  target's: it is a separate image built per target, and no byte of it reaches
  the disk.
- The composefs backend will not read the image out of containers-storage, so
  `skopeo copy` writes it to an OCI layout under `out/oci-cache` and
  `--source-imgref oci:` points the install at that. It needs `skopeo`. This
  used to be a transient local registry the script stood up and tore down.
- A target carrying a module that imports `passwd.hashed-password.*` gets a
  console login: `run` and `spawn` pass a `tect` account through systemd
  credentials, asking for its password without storing it in the image. Set
  `VM_USER` to change the account or `VM_PASSWORD_HASH` to supply a stable
  crypt(5) hash on a run without a terminal. Credentials provision an account
  only on its first boot, so later boots use the password chosen then.
- On the `bootc install to-disk` path, where the target also declares an SSH key
  and its public half has been recorded, the install passes it as a kernel
  command line credential,
  `systemd.set_credential_binary=ssh.authorized_keys.root`, which systemd's
  `provision.conf` reads on the installed machine, because a composefs install
  drops `--root-ssh-authorized-keys`. This is separate from the hypervisor
  credential path.

###### **Subcommands:**

* `build` — convert the built image into a qcow2, raw or iso
* `run` — boot that disk under qemu, building it if missing
* `spawn` — boot a qcow2 or raw disk with systemd-vmspawn



## `tect vm build`

convert the built image into a qcow2, raw or iso

**Usage:** `tect vm build [OPTIONS] [type]`

###### **Arguments:**

* `<type>`

###### **Options:**

* `--target <target>` — what a rebuild builds, and what an iso installs
* `--image <ref>` — the container image to convert, without its tag
* `--tag <tag>` — its tag, else $DEFAULT_TAG, else latest
* `--ram <size>` — memory for the virtual machine
* `--rebuild` — fetch, generate and build the container image first



## `tect vm run`

boot that disk under qemu, building it if missing

**Usage:** `tect vm run [OPTIONS] [type]`

###### **Arguments:**

* `<type>`

###### **Options:**

* `--target <target>` — what a rebuild builds, and what an iso installs
* `--image <ref>` — the container image to convert, without its tag
* `--tag <tag>` — its tag, else $DEFAULT_TAG, else latest
* `--ram <size>` — memory for the virtual machine
* `--rebuild` — fetch, generate and build the container image first



## `tect vm spawn`

boot a qcow2 or raw disk with systemd-vmspawn

**Usage:** `tect vm spawn [OPTIONS] [type]`

###### **Arguments:**

* `<type>`

###### **Options:**

* `--target <target>` — what a rebuild builds, and what an iso installs
* `--image <ref>` — the container image to convert, without its tag
* `--tag <tag>` — its tag, else $DEFAULT_TAG, else latest
* `--ram <size>` — memory for the virtual machine
* `--rebuild` — fetch, generate and build the container image first



## `tect section`

Prints the generated Containerfile module section for one image, the default
image when none is named.

**Usage:** `tect section [image]`

###### **Arguments:**

* `<image>`



## `tect graph`

Prints the default image's capability graph: what provides what, what requires
it, what only orders against it, and what the base already carries.

**Usage:** `tect graph [OPTIONS]`

###### **Options:**

* `--format <md|json>` — markdown holding a mermaid diagram by default, or json



## `tect why`

One module's trust read-out: which targets build it, what it provides and who
requires that, what it requires and what provides it, what it claims to
harden, and where every byte of it came from: the collection it was imported
from and at what pin, whether it has been edited since, what it fetches, and
whether it enables a third-party package repository.

**Usage:** `tect why [OPTIONS] [module]`

Notes:

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
- `why` says plainly when a module was edited since it was imported, and that
  is not an error. Forking one is legitimate; what the record buys is that the
  fork is visible rather than silent. `audit { enforce #true }` is what makes it
  fail.
- A name nothing declares lists the ones that are declared.
- There is no grammar for a `repo` file, so this points at it and prints the
  URLs it found rather than claiming to have understood it.

###### **Arguments:**

* `<module>`

###### **Options:**

* `--format <md|json>` — markdown, the default, or JSON



## `tect coverage`

Every rule the profile an image declares `conforms` to selects, the number a
claim names it by, which of the image's modules claims it, and which module in
the repository or its collections would claim one nothing does. The default
image when none is named, a picker where there is a terminal to pick on.

**Usage:** `tect coverage [OPTIONS] [image]`

Notes:

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

###### **Arguments:**

* `<image>`

###### **Options:**

* `--format <md|json>` — markdown, the default, or json
* `--datastream <file>` — the SSG content the profile is read out of



## `tect plan`

Prints every fact this repository derives, as one JSON document: the images,
each image's targets, and what each target is made of. Read a field out of it
rather than deriving anything from a name.

**Usage:** `tect plan [OPTIONS]`

###### **Options:**

* `--json` — the output is JSON with or without it



## `tect verify`

Re-emits every artifact and byte-compares it against what is committed under
`generated/`, naming what differs, what is missing, and anything under
`generated/` that nothing emits. It runs before every build.

**Usage:** `tect verify`



## `tect summary`

Prints what one target is made of, as a markdown table: every module it builds,
with its description and the options it resolved. This is what a build writes
into its job summary.

**Usage:** `tect summary [target]`

###### **Arguments:**

* `<target>`



## `tect sbom`

Prints the pinned payloads one target carries, as SPDX packages and the
relationships that describe them. A scan of the built image cannot see where a
downloaded asset came from, so this is merged into the SBOM the scan produces.

**Usage:** `tect sbom [target]`

###### **Arguments:**

* `<target>`



## `tect scap`

Reads one scan's report against the datastream it was produced with, and prints
what the two of them say about the target, as markdown: what the modules
claimed and what was measured for each, what the image scores against every
profile the datastream carries, and what stopped passing since the last scan.

**Usage:** `tect scap [OPTIONS] [arf.xml] [COMMAND]`

Notes:

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
  the current pass set behind as the next floor. So one deliberate regression is
  one red run, and no file the user must find and delete.
- `--base-scan` names a pass set a scan of the bare base wrote, in the same
  format `--baseline` writes, and adds a *base alone* column. A claim the base
  already passes is reported not load-bearing, which is a notice and never a
  finding: the module may implement the rule as well, and it applies its
  settings either way. The document records passes only, so a rule missing from
  it is not a rule the base failed.
- Findings are fatal only under `audit { enforce #true }`, like every other
  audit fact, and the report goes to stdout either way.

###### **Subcommands:**

* `content` — print the datastream the target is measured with
* `tailoring` — print the tailoring the target is scanned with
* `rules` — print the rule each benchmark number reaches, one per line

###### **Arguments:**

* `<arf.xml>`

###### **Options:**

* `--target <target>` — the target, else the ungated one
* `--datastream <file>` — the SSG content, else the one `scap content` names
* `--baseline <file>` — the last scan's pass set, read then rewritten
* `--base-scan <file>` — what the bare base passed alone, read only



## `tect scap content`

Prints the datastream the target is measured with, and nothing at all when the
image declares no `conforms`, which is an image asking not to be scanned. This
is what the scan job gates on, and `tect set conforms` is what opens it.

**Usage:** `tect scap content [OPTIONS]`

###### **Options:**

* `--target <target>` — the target, else the ungated one



## `tect scap tailoring`

Prints the XCCDF tailoring a scan of the target runs, as profile
`xccdf_tect_profile_measured`: the declared profile, with every group and rule
selected. Nothing at all when the image declares no `conforms`.

**Usage:** `tect scap tailoring [OPTIONS]`

Notes:

- `--profile '(all)'` scores every variable at its default, so a rule
  remediated to the profile's value reads as failing. The tailoring keeps the
  profile's values and still evaluates every claim outside it.

###### **Options:**

* `--target <target>` — the target, else the ungated one
* `--datastream <file>` — the SSG content, else the one `scap content` names



## `tect scap rules`

print the rule each benchmark number reaches, one per line

**Usage:** `tect scap rules [OPTIONS] [number]...`

###### **Arguments:**

* `<number>`

###### **Options:**

* `--datastream <file>` — the SSG content, else the one `scap content` names



## `tect fetch`

Downloads one payload, verifies it against the hash, and places it by what it
is:

- `file` keeps it
- `tree` unpacks it
- `bin` installs one executable
- `rpm` installs the package, on an rpm family
- `deb` installs the package, on a deb family

**Usage:** `tect fetch <what> <url> <sha256> [target] [extra]...
       fetch [what] [url] [sha256] [target] [extra]... <COMMAND>`

###### **Subcommands:**

* `modules` — fetch every out-of-tree module the images reference

###### **Arguments:**

* `<what>`
* `<url>`
* `<sha256>`
* `<target>`
* `<extra>`



## `tect fetch modules`

Fetches every out-of-tree module the images reference, verifies one whose pin
has a hash, and puts it under `modules/.remote/`.

**Usage:** `tect fetch modules`

Notes:

- A tree already at its pin is left alone; one no image references any more is
  removed.
- It reads the declarations rather than the resolved plan, so it runs before the
  modules it fetches can be read.



## `tect registry`

print where images publish, and under what reference

**Usage:** `tect registry [COMMAND]`

###### **Subcommands:**

* `namespace` — print where images publish
* `ref` — print the full reference one target publishes under



## `tect registry namespace`

Prints where images publish: `$IMAGE_REGISTRY`, else `ghcr.io/<owner>` read off
the github origin remote.

**Usage:** `tect registry namespace`



## `tect registry ref`

Prints the full reference one target publishes under, joining the namespace to
the target's name and tag. The ungated target when none is named.

**Usage:** `tect registry ref [OPTIONS]`

###### **Options:**

* `--target <target>` — the target, else the ungated one
* `--tag <tag>` — the tag, else $DEFAULT_TAG, else latest



## `tect recipe`

Prints the half of an installation recipe the declaration answers, as JSON for
the installer: the reference installed and the reference the installed machine
updates from, plus image properties such as composefs sealing, the boot chain
and bootloader, the root filesystem and whether the target declares a
LUKS-capable initramfs. A successful `tect build` validates that last
declaration against the archive itself. The disk, the account and the encryption
choice are the person's and are not in it.

**Usage:** `tect recipe [OPTIONS]`

Notes:

- The base family settles every derived value, and a family with no answer is
  refused rather than defaulted: a wrong one here is a disk that is erased and
  then does not boot, minutes after the person confirmed.
- `--image` is what installs a local build without making `localhost/...` the
  machine's update origin: the bytes come from it, `targetImgref` stays the
  published reference.
- `hostname` is the published name, and is the one derived value the user is
  expected to replace.

###### **Options:**

* `--target <target>` — the target, else the ungated one
* `--tag <tag>` — the tag, else $DEFAULT_TAG, else latest
* `--image <ref>` — the bytes installed, else the published reference



## `tect os-release`

Writes the image identity the build ARGs carry into `/usr/lib/os-release`.

**Usage:** `tect os-release`



## `tect build-record`

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

**Usage:** `tect build-record`



## `tect validate-image`

Runs every check a built image has to pass. The build passes it the preset files
the enabled modules' overlays ship, and it fails on any the image does not have.

**Usage:** `tect validate-image`



