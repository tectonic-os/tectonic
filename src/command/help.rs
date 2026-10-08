//! These constants hold the prose `--help` prints past the usage line. `docs/cli.md` is
//! rendered from the same strings.

pub(super) const ROOT_NOTES: &str = r#"> [!NOTE]
> In a terminal, a command asks for each answer that its arguments and flags
> did not give. `tect create repo` then shows every answer on a review screen.
> Enter on an answer asks it again, Create writes, and Esc leaves without
> writing.
>
> With `--no-tui`, or with no terminal, a command asks nothing. A typed answer
> comes from its flag, else from its default, else the command fails and names
> the flag. A yes or no question answers no, and a choice keeps the options
> that it opens with."#;

pub(super) const UPGRADE: &str = r#"Replaces this `tect`, and the assets it scaffolds from, with the latest
published release. It prints the running version and the latest one. If they
are the same, or if the running build is newer than the latest tag, then it
stops."#;

pub(super) const UPGRADE_NOTES: &str = r#"> [!NOTE]
> The binary and its assets move together. A binary without its assets
> scaffolds from a stale `assets/` copy and says nothing.
>
> Run as root, it installs to `/usr/local/bin` and
> `/usr/local/share/tectonic/assets`. Run as another user, it installs to
> `~/.local/bin` and `$XDG_DATA_HOME/tectonic/assets`, which is
> `~/.local/share/tectonic/assets` by default. If it cannot write there, then
> it stops and names the other location. It never falls back to it.
>
> It checks the download against the `.sha256` published beside it before it
> changes a file. If the check fails, then the old install stays.
>
> It replaces the assets as a whole, so a file that a release removed does not
> survive.
>
> An `assets` directory beside the binary takes priority over the installed
> assets, so it stops before it downloads. A set `TECT_ASSETS` also takes
> priority, and it warns about that.
>
> Releases exist for x86_64 and aarch64 Linux. On another platform it stops and
> names the platform.
>
> On a machine with no `tect`, the install script does the same job:
> `curl -fsSL https://raw.githubusercontent.com/tectonic-os/tectonic/main/install.sh | sh`."#;

pub(super) const CREATE_REPO: &str = r#"Starts a repository for the user's images. Every other command reads the
`repo.kdl` that it writes, so it runs first. It creates a directory named after
the repository, or uses the directory that `--root` names.

It asks, in order:

1. Name the repository.
2. Choose whether to sync the repository to a host. Then choose the host and
   give the account.
3. On GitHub, choose whether to create the remote repository now.
4. Choose whether to add a first image. Then name it and choose its base.
5. If the base needs modules, choose whether to add them.
6. If the repository syncs to a host, choose the CI workflows. Then choose
   whether images publish and scan only on scheduled builds, and give the daily
   build time.
7. Choose the scripts to keep in `scripts/`.
8. Review every answer, change any of them, and choose Create."#;

pub(super) const CREATE_REPO_NOTES: &str = r#"> [!WARNING]
> The `tectonic-modules` source that `repo.kdl` declares is `unpinned`. Each
> fetch takes the head of its branch, and no hash checks what arrives. To pin
> it, add `version` and `sha256` beside its `url`.

> [!NOTE]
> It runs `git init` and does no other git step. It prints the commands for the
> first commit, the remote and the push.
>
> On GitHub, it offers to create the remote repository with `gh`. If `gh` is
> missing or logged out, then it says what to do.
>
> A repository cannot sit inside another repository, so it refuses such a
> directory."#;

pub(super) const CREATE_IMAGE: &str = r#"Adds an image to the repository, in a new image file at the root. The file
name derives from the image name, so "My Desktop" writes
`my-desktop.image.kdl`.

It asks, in order:

1. Name the image. The repository name is the default.
2. Choose the base from the catalog. If `--base` names a base outside the
   catalog, then choose its family.
3. If the base needs modules that the image does not list, such as the family
   adapter, choose whether to add them from the declared collections."#;

pub(super) const CREATE_IMAGE_NOTES: &str = r#"> [!NOTE]
> A base outside the catalog asks for its family and writes no `provides`.
> `tect check` then reports each capability that no module provides.
>
> If the repository declares no `default-image`, then a second image adds one
> to `repo.kdl` that names the first image. A bare `tect build` then builds what
> it built before.
>
> To offer the base's modules, it reads the declared collections, and fetches a
> collection that is not on this machine. With no terminal, it fetches
> nothing.
>
> The base's modules are the family adapter and a provider of each capability
> that the base requires. The family adapter is the module that `supports` the
> family and `provides "build-environment"`."#;

pub(super) const CREATE_FLAVOUR: &str = r#"Adds a flavour to an image. A flavour is a second build of the image with more
modules, published as `<image>-<flavour>` beside the image. This command
declares the flavour in the image's `flavours` block. To add a module to the
flavour, run `tect create module` or `tect import module` and choose the
flavour.

It asks, in order:

1. Name the flavour.
2. Choose the image that publishes it."#;

pub(super) const CREATE_FLAVOUR_NOTES: &str = r#"> [!NOTE]
> It writes neither `default` nor `pr-build`. Each one changes what a build
> does, so the user writes it.
>
> It refuses a name that the image already declares, and `none`, which names
> the image's own build."#;

pub(super) const CREATE_MODULE: &str = r#"Writes a new module, `modules/<name>/module.kdl`, and offers to list it in an
image. The name can be a path, so `apps/firefox` writes
`modules/apps/firefox/module.kdl`.

It asks, in order:

1. Name the module.
2. Optionally describe it.
3. Optionally restrict it to named base families.
4. Choose whether it installs packages, then name them.
5. Choose the images and flavours that list it."#;

pub(super) const CREATE_MODULE_NOTES: &str = r#"> [!NOTE]
> With no `supports` declaration, a module supports every base family. It
> writes packages outside a family gate so every supported family installs
> them. A package name that differs by family goes in a `family` block.
> The build installs them through the family adapter, with `dnf install -y` on
> Fedora and RHEL, and with `apt-get update`, `apt-get install -y` and
> `apt-get clean` on Debian and Ubuntu.
>
> An image and one of its own flavours cannot both list the module. The image
> entry already reaches every flavour of that image.
>
> No image is an answer. The module exists, and no image lists it yet. Leaving
> the picker writes nothing."#;

pub(super) const IMPORT_MODULE: &str = r#"Lists a module from a collection in an image, without a copy in the
repository. The image's `source` block names the module, and a fetch puts it in
`modules/.remote/`. Use it for a module that a collection maintains.

It asks, in order:

1. Choose the modules.
2. Choose the images and flavours that list them.
3. Choose whether to add the modules that they require.
4. Choose whether to generate the CI workflows that they make runnable.
5. If they claim benchmark rules, choose a profile to measure the image
   against."#;

pub(super) const IMPORT_MODULE_NOTES: &str = r#"> [!WARNING]
> A pinned collection is downloaded once and checked against its hash. An
> `unpinned` collection is downloaded each time with no check, and
> `audit { enforce #true }` refuses it.

> [!NOTE]
> If `repo.kdl` declares no `sources`, the command uses the default module
> collection supplied with `tect` and adds that declaration to `repo.kdl`.
>
> A bare name is searched for in every collection. `<owner>/<name>` picks
> between two collections that both hold it.
>
> It refuses a module that the image already lists.
>
> If the user declines a required module, then the file stays valid, and
> `tect check` names the import that would satisfy it."#;

pub(super) const COPY_MODULE: &str = r#"Copies a module from a collection into `modules/<name>`, and records where it
came from in `provenance.kdl`. The repository owns the copy, so no fetch or pin
changes it. Use it for a collection module that the user wants to change.

It asks the same questions as `tect import module`."#;

pub(super) const COPY_MODULE_NOTES: &str = r#"> [!NOTE]
> If `repo.kdl` declares no `sources`, the command uses the default module
> collection supplied with `tect` and adds that declaration to `repo.kdl`.
>
> A required module that it brings in is copied too.
>
> No image is an answer. The copy is in the repository whether an image lists
> it or not.
>
> If `modules/<name>` exists, then it refuses and names the collection that the
> existing module came from."#;

pub(super) const CREATE_GIT: &str = r#"Puts the directory under git control, so a hand-written repository is under
version control before its first commit. It writes the `.gitignore` that
ignores `keys/private/`, `out/` and `modules/.remote/`, unless one is already
there.

It refuses a directory that git already tracks, because a repository does not
nest."#;

pub(super) const CREATE_GIT_NOTES: &str = r#"> [!NOTE]
> `tect create repo` already does this for a repository it scaffolds. This
> command is for a directory written by hand or copied from somewhere, and it
> leaves every file that is already there alone."#;

pub(super) const CREATE_SCRIPTS: &str = r#"Copies a script that `tect` supplies into `scripts/`, where the repository
owns it. Git tracks the copy, the user can review and change it, and a `tect`
upgrade does not replace it. The scripts are:

- `Containerfile.skeleton`, the Containerfile around the generated module layers
- `lint.sh`, `tect.sh`, `smoke.sh` and `vm.sh`, which `tect generate` otherwise
  writes into `generated/scripts/`

It asks for one step:

1. Choose the scripts. The list holds only the scripts that `scripts/` does
   not hold yet."#;

pub(super) const CREATE_SCRIPTS_NOTES: &str = r#"> [!NOTE]
> Run `tect generate` after it. Every generated script and workflow then calls
> the copy in `scripts/`.
>
> A kept script that still matches what `tect` supplied is updated to call the
> new copies. A kept script that the user changed stays as it is.
>
> `tect check` says when a kept script differs from what the running release
> supplies.
>
> `tect create repo` asks the same question, with the skeleton chosen."#;

pub(super) const CREATE_KEY: &str = r#"Generates a key that a module in the repository declares. The public half goes
under `keys/public/`, at the path it has in the image. The private half goes
under `keys/private/`, which git ignores.

It makes each kind with its own tool:

- `cosign` makes the key pair that signs a published image. Set the private
  half as the `SIGNING_SECRET` repository secret. `cosign` must be installed.
- `openssl profile="module-signing"` makes the Secure Boot certificate that
  signs the kernel and its modules. Set the private half as the `MOK_PRIVKEY`
  repository secret, and `$MOK_KEY_PATH` points a local build at it. It
  writes the `format` that the module declares, and `sign-file` and `mokutil`
  read DER.
- `openssl profile="pcr-signing"` makes the RSA pair that signs the PCR 11
  policy of a sealed UKI. Set the private half as the `PCR_PRIVKEY` repository
  secret. A local build reads it through
  `tect build --secret pcr_privkey=<path>`.
- `openssl profile="tls-ca"` makes a certificate authority that a TLS client
  trusts. It needs no repository secret, because a build issues its
  certificates.
- `ssh-keygen` makes the ed25519 key pair that the user logs in with. The key
  has no passphrase. The image ships the public half, and no build reads the
  private half.

It asks for one step:

1. Choose the kind."#;

pub(super) const CREATE_KEY_NOTES: &str = r#"> [!WARNING]
> It never replaces an existing key, because a private half cannot be
> recovered.

> [!NOTE]
> Each machine enrols a `module-signing` certificate once with
> `mokutil --import`. Until then, the modules it signs do not load.
>
> If `.gitignore` does not cover `keys/private/`, then it says so and does not
> edit the file.
>
> If no module declares the kind, then it names a module in the declared
> collections that does, and the `tect import module` line that adds it."#;

pub(super) const SET_KEY: &str = r#"Records the public half of a key that the user already holds, in place of
generating one. Use it for an existing cosign key, MOK certificate or SSH key.
It writes to the path that the module's `public` declaration gives.

It asks, in order:

1. Choose the kind.
2. Give the file to read."#;

pub(super) const SET_KEY_NOTES: &str = r#"> [!WARNING]
> It never replaces an existing key.

> [!NOTE]
> It takes the public half only. The private half stays with the user, or in a
> CI secret.
>
> It checks the form of the file first, so a wrong form stops here and no later
> build fails on it. Each kind takes one form:
>
> - `cosign` takes a PEM public key.
> - `module-signing` takes a PEM or DER certificate.
> - `pcr-signing` takes a bare PEM public key.
> - `ssh-keygen` takes an OpenSSH key line."#;

pub(super) const SET_WORKFLOWS: &str = r#"Chooses the CI workflows that the repository generates, and writes the choice
into `workflows` in `repo.kdl`. Run `tect generate` next to write the workflow
files. A workflow that the repository cannot run shows the reason, and the user
cannot choose it. With no terminal, it changes nothing and says to edit
`repo.kdl`.

It asks, in order:

1. Choose the workflows.
2. Choose whether images publish only on scheduled builds.
3. Choose whether image scans run only on scheduled builds.
4. If a chosen workflow has a schedule, give the daily build time in UTC."#;

pub(super) const SET_WORKFLOWS_NOTES: &str = r#"> [!NOTE]
> No workflow removes the `workflows` block, and the repository then generates
> no CI. Leaving the picker changes nothing.
>
> Each schedule is an offset from the daily build time, so a new daily time
> moves every schedule."#;

pub(super) const SET_LIBRARY: &str = r#"Adds a library to the `sources` block in `repo.kdl`: the default library of
that kind, or a source the user types. A source of the same kind and alias is
replaced, so the declaration moves to what the user chose.

It asks, in order:

1. Choose the default library of that kind, or another source.
2. For another source, give its alias, its HTTPS Git URL and the directory
   inside the repository."#;

pub(super) const SET_CONFORMS: &str = r#"Chooses the benchmark profile that a scan measures an image against, and writes
it into the image's `conforms`. A `conforms` turns on the image scan for every
build. With no terminal, it changes nothing and says to write the line into the
image file.

It asks, in order:

1. If the repository holds several images, choose one.
2. Choose the profile.
3. Choose whether to import the collection modules that claim rules of the
   profile that nothing in the image claims yet."#;

pub(super) const SET_CONFORMS_NOTES: &str = r#"> [!NOTE]
> A `conforms` is not a claim that the image passes. It names what the image is
> measured against. If the repository sets `audit { enforce #true }`, then a
> failed rule fails the build.
>
> A second run replaces the profile.
>
> If no `--datastream` is given and no SCAP content is installed for the
> image's family, then it stops and names `scap-security-guide`.
>
> The import offer covers collection modules only. A module in the repository
> needs a line in the image's `modules` block, and `tect check` names it."#;

pub(super) const SET_CLAIMS: &str = r#"Chooses the benchmark rules that a module in the repository claims to cover,
and writes their numbers into its `satisfies` block. A claim says what the
module supplies, and `tect scap` measures whether the built image keeps it.

It asks, in order:

1. Choose the profile that the rules come from.
2. Choose the rules that the module claims."#;

pub(super) const SET_CLAIMS_NOTES: &str = r#"> [!NOTE]
> A group chooses every rule under it. The file holds the number of each rule,
> never the number of a group, so a claim does not grow when the content does.
>
> A rule with no number of its own cannot be claimed. The list leaves it out
> and counts it above the question.
>
> A claim on a rule that the chosen profile does not select stays, so a second
> profile does not remove what the first one wrote.
>
> No rule removes the `satisfies` block. Leaving the picker changes nothing.
>
> A module that declares `supports` uses the installed content for its first
> named family by default. A module with no `supports` declaration has no
> single default, so name `--datastream`. If that content is not installed, the
> command stops and names `scap-security-guide`.
>
> The benchmark node that holds the numbers does not change what they mean. A
> number resolves against the datastream, never against the benchmark name."#;

pub(super) const CHECK: &str = r#"Reads every file in the repository and reports each problem at the line that
caused it. Run it after an edit. The last line counts the images, the modules,
the flavours and the listed modules that the base already provides."#;

pub(super) const CHECK_NOTES: &str = r#"> [!NOTE]
> It also reports these findings, which do not change the exit code:
>
> - A collection describes a base differently from the catalog.
> - A collection is `unpinned`.
> - An image `conforms` to a profile, and no listed module claims a rule of it.
> - A `module.kdl` sits inside the directory of another collection module.
>
> With `--datastream`, it also counts the rules of each declared profile that
> no listed module claims, and names the modules that would claim them.
> `tect scap content` prints the path to give it."#;

pub(super) const GENERATE: &str = r#"Writes the build files, and the CI workflows that `repo.kdl` names. Run it
after a change to a KDL file. `tect build` refuses build files that are not
current. It fetches the imported modules first, so the build files match the
collections. It writes:

- `generated/<image>/Containerfile`, one for each image
- `generated/<image>/modules/<module>.sh`, the build script of each module
- `generated/<image>/finalize.sh`
- `generated/<image>/graph.md` and `graph.json`, the capability graph
- `generated/plan.json`
- `generated/seed.kdl`, if `repo.kdl` names a seedable image
- each workflow that `workflows` names, under `.github/workflows/`"#;

pub(super) const GENERATE_NOTES: &str = r#"> [!NOTE]
> It clears `generated/` first, so the files of a removed image or module go
> too. It removes only the workflows that `tect` ships, and keeps the
> repository's own.
>
> A terminal gets a tree of the files, and a pipe gets one path per line.
>
> The Containerfile copies `plan.json` into the image at
> `/usr/share/tectonic/manifest.json`, so a built image can say what it is made
> of.
>
> It resolves the base tag to a digest once, and passes it as the `BASE` build
> argument. If `$BASE` is set, then it uses that value as already resolved."#;

pub(super) const BUILD: &str = r#"Builds one target with the container backend. A target is `<image>` for the
image, or `<image>/<flavour>` for a flavour. With no target, it builds the
default image. It runs `tect verify` first, so it builds only from build files
that are current."#;

pub(super) const BUILD_NOTES: &str = r#"> [!NOTE]
> It fetches nothing and writes no build file. Run `tect fetch modules` and
> `tect generate` first, or `tect vm run --rebuild` to run all three.
>
> `$TAGS` adds tags and `$LABELS` adds OCI labels. `$IMAGE_VERSION` is stamped
> into the image, and is today's date in UTC by default.
>
> A UKI target builds in two passes. The first builds the image, and the second
> seals it with the storage digest of the first. The digest comes from the local
> buildah store, so it refuses `--backend buildx` for a UKI target. It checks the
> sealed image against the embedded digest before it applies a tag, so a failed
> check publishes nothing."#;

pub(super) const VM: &str = r#"Turns the built image into a disk and boots it, to try the image before it is
published. `<type>` is `qcow2`, `raw` or `iso`, and a terminal asks for it.
`build` makes the disk. `run` boots it under qemu, and makes it first if it is
missing. `spawn` boots it with systemd-vmspawn, which cannot boot an `iso`."#;

pub(super) const VM_NOTES: &str = r#"> [!NOTE]
> `--rebuild` runs `tect fetch modules`, `tect generate` and `tect build` in
> that order first. Without it, nothing is fetched, written or built, and the
> existing disk boots.
>
> It runs `vm.sh`, from `scripts/` if the repository keeps a copy, else from
> `generated/scripts/`. The script asks for sudo, because it reads the image out
> of rootful podman.
>
> Fedora and RHEL disks use bootc-image-builder, at the size that
> `disk_config/disk.toml` sets. Debian and Ubuntu disks use
> `bootc install to-disk`, at `DISK_SIZE`, which is 20G by default, and need
> `skopeo`.
>
> An `iso` is a live installer for the target, which installs from the recipe
> that `tect recipe` prints. It needs a published reference, because the
> installed machine updates from `$IMAGE_REGISTRY`. `run iso` keeps the installed disk
> under `out/bootiso/storage/`, so a reboot tests what was installed.
>
> If a module imports `passwd.hashed-password.*`, then `run` and `spawn` add a
> `tect` account for the console and ask for its password. `VM_USER` changes the
> account, and `VM_PASSWORD_HASH` gives a crypt(5) hash for a run with no
> terminal. The account is set on the first boot only."#;

pub(super) const SECTION: &str = r#"Prints the generated Containerfile module section of one image, or of the
default image if none is named."#;

pub(super) const GRAPH: &str = r#"Prints the capability graph of the default image. The graph shows what provides
each capability, what requires it, what only orders against it, and what the
base already carries."#;

pub(super) const COVERAGE: &str = r#"Prints each rule of the profile that an image `conforms` to, with the module
that claims it. If no module claims a rule, then it names a module in the
repository or its collections that would. It uses the default image if none is
named, and a terminal picks one."#;

pub(super) const COVERAGE_NOTES: &str = r#"> [!NOTE]
> It reads no scan. It says what is claimed, and `tect scap` measures what
> passes.
>
> It reads the content that `--datastream` names and never probes the host, so
> the result does not depend on the machine.
>
> A terminal gets a table, with each rule that nothing claims in red. A pipe
> gets markdown, so `tect coverage > report.md` exports it.
>
> A rule with an empty `Number` cell has no number that a `satisfies` can name,
> so no module can claim it."#;

pub(super) const PLAN: &str = r#"Prints every fact that the repository derives as one JSON document. The
document holds the images, the targets of each image, and what each target is
made of. A script reads a field from it, and derives nothing from a name."#;

pub(super) const VERIFY: &str = r#"Writes every build file again in memory, and compares each one byte for byte
with what `generated/` holds. It names each file that differs, each file that is
missing, and each file under `generated/` that nothing writes. Every build runs
it first."#;

pub(super) const SUMMARY: &str = r#"Prints a markdown table of what one target is made of. Each row is a module
that the target builds, with its description and the options it resolved. A CI
build writes the table into its job summary."#;

pub(super) const SBOM: &str = r#"Prints the pinned payloads of one target, as SPDX packages and their
relationships. A scan of the built image cannot see where a downloaded asset
came from, so CI merges this into the SBOM that the scan produces."#;

pub(super) const FETCH_MODULES: &str = r#"Fetches every module that the images import, checks each one that has a hash,
and puts it under `modules/.remote/`. `tect generate` and the build read the
modules from there."#;

pub(super) const FETCH_MODULES_NOTES: &str = r#"> [!NOTE]
> A module already at its pin stays. A module that no image references any more
> is removed.
>
> It reads the declarations, and not the resolved plan, so it runs before the
> modules it fetches can be read."#;

pub(super) const SCAP: &str = r#"Reads the report of one scan against the datastream that produced it, and
prints what they say about the target as markdown. The report shows what each
module claimed and what was measured, the score against each profile of the
datastream, and what stopped passing since the last scan."#;

pub(super) const SCAP_NOTES: &str = r#"> [!NOTE]
> The datastream maps each benchmark number to a rule. A number that maps to no
> rule is a fault in the declaration, and the image is not at fault. A declared
> profile that the datastream does not carry is a finding.
>
> A claimed rule that the image fails names the module that claimed it. If
> another module replaced a file of the claiming module, then it names that
> module too.
>
> `--baseline` keeps a pass set. A rule that passed the last scan and fails now
> is a finding, and each run writes the new pass set, so a deliberate
> regression is one failed run.
>
> `--base-scan` adds a column for a scan of the bare base. A claim that the base
> already passes is reported as a notice, never as a finding.
>
> A finding fails the command only under `audit { enforce #true }`. The report
> goes to stdout either way."#;

pub(super) const SCAP_CONTENT: &str = r#"Prints the path of the datastream that the target is measured with. If the image
declares no `conforms`, then it prints nothing, and the CI scan job skips the
image. `tect set conforms` writes a `conforms`."#;

pub(super) const SCAP_TAILORING: &str = r#"Prints the XCCDF tailoring that a scan of the target runs. The tailoring defines
the profile `xccdf_tect_profile_measured`, which is the declared profile with
every group and rule selected. If the image declares no `conforms`, then it
prints nothing."#;

pub(super) const SCAP_TAILORING_NOTES: &str = r#"> [!NOTE]
> `--profile '(all)'` scores every variable at its default, so a rule set to the
> profile's value reads as failing. The tailoring keeps the profile's values and
> still checks every claim outside the profile."#;

pub(super) const REGISTRY_NAMESPACE: &str = r#"Prints where the images publish. If `$IMAGE_REGISTRY` is set, then it prints
that value. Otherwise it prints `ghcr.io/<owner>`, with the owner read from the
GitHub origin remote."#;

pub(super) const REGISTRY_REF: &str = r#"Prints the full reference that one target publishes under. The reference joins
the namespace, the name of the target and its tag. With no target, it uses the
default image."#;

pub(super) const RECIPE: &str = r#"Prints the part of an install recipe that the repository answers, as JSON that
the installer reads. The recipe holds the reference to install, the reference
the machine updates from, and properties of the image. The properties are the
boot chain, the bootloader, the root filesystem and whether the initramfs can
unlock LUKS. The disk, the account and the encryption choice belong to the
user, so the recipe holds none of them."#;

pub(super) const RECIPE_NOTES: &str = r#"> [!NOTE]
> The base family settles each value. For a family with no measured answer it
> stops, because a wrong value erases a disk that then does not boot.
>
> `--image` installs a local build while the machine still updates from the
> published reference.
>
> `hostname` is the published name. It is the one value that the user is
> expected to replace."#;

pub(super) const WHY: &str = r#"Prints what one module is and where every byte of it came from. It lists the
targets that build it, what it provides and requires, and what it claims to
harden. It also lists the collection and pin it came from, whether it was
edited since, what it fetches, and whether it adds a package repository."#;

pub(super) const WHY_NOTES: &str = r#"> [!NOTE]
> In a repository it reads the resolved plan. On a booted image it reads the
> two documents that the build baked into the image, and it also prints the
> source repository, the commit and the `git clone` that reaches them.
>
> On a booted image, it refuses a document written for a schema version that
> this `tect` does not read, and names both versions. `tect plan --json` still
> prints the document.
>
> A module edited since its import is said plainly, and it is not an error.
> `audit { enforce #true }` makes it fail.
>
> `tect` does not read a `repo` file, so it points at the file and prints the
> URLs it found."#;

pub(super) const OS_RELEASE: &str = r#"Writes the image identity, which the build arguments carry, into
`/usr/lib/os-release`. The generated Containerfile runs it."#;

pub(super) const BUILD_RECORD: &str = r#"Writes `/usr/share/tectonic/build.json`, the record of what the build resolved.
The baked `manifest.json` beside it holds what the repository declared. The
record holds the digest of the base, the commit of each cloned asset, the source
commit, the `tect` release, the target, the content hash of each module, and
whether enforcement was on.

No file under `generated/` holds the record, so `tect verify` never compares
it. The record changes with each build."#;

pub(super) const FETCH: &str = r#"Downloads one payload, checks it against its hash, and places it by its kind.
It runs inside a build:

- `file` keeps the download
- `tree` unpacks it
- `bin` installs one executable
- `rpm` installs the package, on an rpm family
- `deb` installs the package, on a deb family"#;

pub(super) const VALIDATE_IMAGE: &str = r#"Runs every check that a built image has to pass. The build gives it the preset
files of the enabled modules, and it fails on each one that the image does not
have."#;
