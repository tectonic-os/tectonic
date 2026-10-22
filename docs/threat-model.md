# Threat model

This page is for a security team that is deciding whether to build a fleet
with `tect`. It names each trust boundary, the control at that boundary, the
risks the project accepts and the gaps that are still open. Each control cites
the file that enforces it. The model covers three repositories: `tectonic`
(the `tect` CLI), `installer` (`tect-installer`) and `common`, the library
that both link.

Report a vulnerability as the [security policy](https://github.com/tectonic-os/tectonic/security/policy) says.

## What an adopter trusts

The image repository is trusted. Its modules run as root inside the build
containers, and so does every module that `tect import module`, `tect create
repo`, `tect create image` or `tect set conforms` copied into it from a
collection.
`tect` controls where a module comes from and records what it was. `tect` does
not sandbox what a module does. The review of the repository and its pins is
the control for that code.

## Trust boundaries

### The image repository and `repo.kdl`

- `audit { enforce #true }` turns each provenance check into an error in the
  command that runs it. The [`repo.kdl`](schema/repo.md#audit-enforce) page
  lists each check and its command. `src/provenance/mod.rs` `enforce_build`
  refuses a build whose base did not resolve to a digest, and a build from a
  tree with no commit.
- Every command that reads the resolved images refuses an imported module
  that tracks its library and no longer matches its entry in
  `modules/modules.lock.kdl` (`src/provenance/lock.rs` `check`, run by
  `src/lib.rs` `load`). The refusal does not depend on `audit`, and `tect
  build` runs it through `tect verify`. `tect import`, `tect create` and
  `tect set` do not refuse, so the user can still repair the repository.
  An untracked module belongs to the repository and is not refused.
- The `security-policy { network }` rule decides which generated module steps
  reach the network. `src/emit/containerfile.rs` runs a closed step with
  `--network=none`. If the repository declares no rule, then every step
  reaches the network.
- A module's `Containerfile.inc` fragment is copied into the build as written
  (`src/emit/containerfile.rs` `fragment`), so the network rule does not reach
  its own `RUN` lines. If the script rule is not `allow`, then `tect check`
  names each module that ships a fragment (`src/provenance/mod.rs`
  `ungoverned`).
- Module secrets enter a step as BuildKit `--mount=type=secret`
  (`src/emit/containerfile.rs`), so no secret is copied into a layer.

### Fetched collections

A collection is a Git repository of modules or base-image catalogs that
`repo.kdl` names under `sources`.

- `src/parse/remote.rs` refuses a source URL that is not `https://`, and a
  `sha256` with no `version`. A `dir` source names a local directory and is
  never fetched.
- A `sha256` pins the canonical `git archive` of the selected version.
  `src/import.rs` `fetch_git` refuses an archive whose hash differs.
- Pinning is optional. A source with no `sha256` is unpinned, and every fetch
  of it takes whatever its ref holds at that time. `tect check` warns about
  each unpinned source (`src/dispatch.rs`).
- An image lists only the modules under `modules/`, and `check`, `generate`
  and `build` read no collection. The image grammar in `src/parse/image.rs`
  refuses a `source` block, and `parse_entry` refuses a `pin`. A collection
  reaches the image only through a copy into `modules/`.
- Under enforcement, `tect` refuses a copy from an unpinned module collection
  (`src/import.rs` `verified`), whichever command makes it. An unpinned
  base-image library is still fetched (`src/base.rs` `fetch`). See *Known
  gaps*.
- A collection is never signature-verified. The hash pin and the review of
  `repo.kdl` are the controls. See *Accepted risks*.
- A module copy refuses a symbolic link (`src/init.rs` `copy_below`), so a
  collection cannot copy a file from outside itself into `modules/`. `tect
  fetch modules` stages the copy first, so a refused copy leaves the module
  as it was.
- The `update-modules` workflow runs `tect fetch modules` with a token that
  can push a branch and open a pull request. What it copies reaches `main`
  only through the review of that pull request.

### Assets a module downloads

An `asset` in `module.kdl` pins a download that the module's own script
fetches while its layer builds.

- An asset whose URL ends in `.git` is cloned, and its commit is the
  verifier. On the build host, `tect` resolves that commit with
  `protocol.ext.allow=never` (`src/provenance/build.rs` `clone_commit`), so the
  resolution cannot run a command through the `ext::` transport.
- The clone itself runs in the module's layer, as trusted module code, under
  Git's own protocol policy. An asset URL is not held to `https://`.
- `tect build` records the resolved commit of each cloned asset in the build
  record. If the commit does not resolve, then `tect build` names the asset
  and leaves it out of the record. Under enforcement, it refuses the build
  (`src/provenance/mod.rs` `enforce_build`).

### The `tect` binary a build runs

- `tect-version` in `repo.kdl` pins the release that `tect.sh` fetches. With
  `sha256=`, the tarball is held to that hash. Without it, `tect.sh` checks
  the `.sha256` published beside the tarball, which proves only that the
  download is complete. `tect check` warns about a pin with no `sha256=`
  (`src/dispatch.rs`).
- If `repo.kdl` declares no `tect-version`, then `tect.sh` resolves the latest
  release on each run (`assets/scripts/tect.sh`). A build then takes a new
  release as soon as it is published, and `tect check` does not warn. See
  *Known gaps*.
- `tect upgrade` runs only when the user runs it. It fetches the tarball and
  the `.sha256` from the same GitHub release (`src/upgrade.rs`). See
  *Accepted risks*.
- Each release of `tect` and `tect-installer` builds `--locked` from the
  committed `Cargo.lock`, embeds its dependency list with `cargo auditable`,
  ships a CycloneDX SBOM per tarball and carries a GitHub provenance
  attestation (`.github/workflows/release.yml` in each repository).

### The build host

- The generated workflows start from `permissions: {}`, grant each job its
  own permissions, and pin every action to a commit SHA
  (`assets/.github/workflows/build.yml`).
- The generated workflow runs `tect build` as root, because every later step
  reads the root container store (`assets/.github/workflows/build.yml`). A
  local `tect build` can run rootless.
- A UKI build runs two containers from the image under build
  (`src/build.rs` `build_uki`). The composefs digest runs the image's own
  `bootc` with `--privileged` and the host's container storage mounted
  (`src/build.rs` `digest_argv`). In the generated workflow that container is
  privileged root on the runner. The sealed image's validation runs under
  its SELinux confinement with no `--privileged`. It mounts the build's own
  copy of the `tect` binary read-only with `z`, which relabels that one file
  (`src/build.rs` `validate_argv`).
- The local VM and media scripts run outside the pinned workflow path.
  `assets/scripts/vm.sh` runs `bootc-image-builder:latest` and an untagged
  `qemux/qemu` image with `--privileged --pull=newer`, so a moved tag runs as
  root on the user's machine. `BIB_IMAGE` replaces the builder image.

### The registry

The generated build pushes the image under the `unverified` tag alone. It
signs the image and its SPDX SBOM attestation with `cosign` and the
repository's key, and verifies both against
`keys/public/etc/pki/containers/cosign.pub`. Only then does it copy the digest
to the published tags (`assets/.github/workflows/build.yml`). If the
verification fails, then no published tag moves. The verification passes
`--insecure-ignore-tlog=true`, so a signature does not need a transparency log
entry.

### Installer media and the installer

- The media fetches `tect-installer` by version and holds it to a sha256
  per architecture (`src/emit/installer.Containerfile`).
- The media carries the image in its own container storage. The installer
  runs each check of the image with `--net=none` (`installer/src/boot.rs`).
- If the media's install recipe names a UKI chain, then the installer refuses
  the image unless the image carries the signed-chain marker
  (`installer/src/boot.rs` `require_signed_boot_chain`). The check runs before
  the first write to the disk (`installer/src/run.rs` `run`).
- The media records the ID of the image it was built to install
  (`assets/scripts/vm.sh`). Before the first write to the disk, the installer
  refuses an image that the store holds under another ID
  (`installer/src/run.rs` `require_built_image`). The confirmation screen
  shows the short ID. A hand-written payload may record no ID, and the
  installer then says that the identity is unchecked.
- In both cases the installer then reads the whole image back out of the
  store. That read checks each layer file against its layer's record, so an
  altered byte refuses the install.
- The image ID and the image are both on the media, so a rewrite of the whole
  media passes the check. See *Known gaps*.
- The install runs the image's own `bootc` with `--privileged`, the host PID
  namespace and `/dev` mounted (`installer/src/run.rs` `bootc_command`). That
  `bootc` comes from the image, so installing the image trusts it with the
  target machine.

### The installer and the partition table

The installer runs as root. It reads the partition table through
`libfdisk.so.1`, which it opens read-only (`installer/src/fdisk.rs`). It
writes the table by passing a script built from the user's answers to the
`sfdisk` binary (`installer/src/table.rs` `sfdisk_script`). The base of the
media supplies both, because `sfdisk` links `libfdisk`. Each `unsafe` block
carries a `// SAFETY:` comment, and the `[lints]` table in
`installer/Cargo.toml` denies one without it.

### The dependency supply chain

- `lint.sh` checks bans, licences and sources against each repository's
  `deny.toml` with `cargo deny`. CI also checks advisories on every pull
  request, every push to `main` and every week (`.github/workflows/ci.yml`).
- Every cargo command in `lint.sh` that resolves the dependency graph passes
  `--locked`, so CI builds the committed lock and never resolves around it.
- In the three tool repositories, Renovate proposes a release only after it
  has stood for seven days (`.github/renovate.json5`). The Renovate
  policy that `tect create repo` writes into an image repository's `repo.kdl`
  has no such wait. See *Known gaps*.

## Accepted risks

Each risk is accepted for the reason given. The last column names what would
reopen it.

| Risk | Why it is accepted | What reopens it |
| --- | --- | --- |
| A collection is pinned by hash and never signature-verified. | The hash binds the exact tree the reviewer read, and the review of `repo.kdl` approves the pin. | A collection publisher that signs its releases, or an adopter that needs to trust a publisher instead of a tree. |
| A source with no `sha256` follows its ref. | Pinning is optional, so a simple repository can follow its library. `tect check` warns about each one. | An adopter that needs a `repo.kdl` rule that requires every source to be pinned. |
| `tect upgrade` trusts a `.sha256` from the same release as the tarball. | No pin exists before the tool does. The GitHub attestation can be checked with `gh attestation verify`. | A signed release manifest. |
| The composefs digest runs the image under build with `--privileged`. | The image's own `bootc` computes the digest from the container storage. The image is already trusted as the repository's own code. | An unprivileged digest command upstream. |
| The install runs the image's own `bootc` privileged on the target machine. | The image is what the user chose to install, so its `bootc` is trusted with the machine. | None. |

`tect` sends no telemetry and runs no background update.

## Known gaps

Each gap is current behaviour that a control does not yet cover.

- Enforcement does not refuse an unpinned base-image library.
- A repository with no `tect-version` takes each new `tect` release on its
  next build, with no warning from `tect check`.
- A `Containerfile.inc` fragment's `RUN` lines run outside the network rule.
- The Renovate policy that `tect create repo` writes into `repo.kdl` merges a
  pin update with no wait after the release.
- A rewrite of the whole installer media, recorded ID included, passes the
  installer's check.

## Air-gapped builds

These parts work with no network:

- a `dir` source, which reads a local collection;
- the sources cache, which holds each pinned collection by its pin;
- `tect check` and `tect generate`, which read the repository's own modules;
- installer media that carries the image in its own storage.

These parts need the network:

- `tect import module` from a Git source, and each new fetch of an unpinned
  one;
- a run of `tect.sh` in a repository with no `tect-version`;
- a registry pull of the base image, and the digest lookup of a base named by
  tag (`src/provenance/build.rs` `base`);
- the commit lookup of each cloned asset;
- a module step that the network rule leaves open.
