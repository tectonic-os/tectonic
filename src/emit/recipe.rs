//! The installation recipe one target installs from.
//!
//! An installer asks a person for the disk, the account and the encryption.
//! Everything else is a property of the image and is derived from the
//! declaration here: a wrong answer erases a disk that then does not boot.

use crate::emit::plan::{of_target, provides};
use crate::model::image::{Layout, List};
use common::json::Json;

pub const LUKS_INITRAMFS: &str = "luks-initramfs";

/// What the base family settles: two `bootc install` flags, the root
/// filesystem the first of them forces, and the group an administrator is
/// created in.
#[cfg_attr(test, derive(Debug, PartialEq))]
struct Family {
    composefs: bool,
    generic: bool,
    filesystem: String,
    /// Naming the other family's group as well is not a hedge that covers
    /// both: `useradd` refuses the whole call when any listed group is
    /// missing, so a two-group list fails on every target.
    admin: String,
}

/// `None` for a family with no answer here, which is what stops a guess
/// reaching a partition table.
fn family(name: &str) -> Option<Family> {
    Some(match name {
        // Both ship bootupd, so `--generic-image` — which skips the bootupd
        // check `bootc install` aborts on — stays off, and nothing seals the
        // deployment.
        "fedora" | "rhel" => Family {
            composefs: false,
            generic: false,
            filesystem: "xfs".into(),
            admin: "wheel".into(),
        },
        // Debian packages no bootupd, so the install aborts without
        // `--generic-image`. The sealed composefs deployment needs fs-verity:
        // xfs has none and drops into a dracut emergency shell, and sealed
        // btrfs fails to mount.
        "debian" | "ubuntu" => Family {
            composefs: true,
            generic: true,
            filesystem: "ext4".into(),
            admin: "sudo".into(),
        },
        _ => return None,
    })
}

/// The five settled, and the bootloader beside them: four from the family
/// that knows them or from the image that declared them, and the bootloader
/// from the image alone, which `tect create image` writes off the base's row.
/// `None` when nothing answers, which is what stops a guess reaching a
/// partition table.
///
/// **Declaring is not guessing.** A family this project has measured answers
/// the four for an image that declares none of them. An image on any other
/// base answers for itself, in full or not at all: half an answer is the one
/// state that would reach `bootc install` on a default nobody chose, and
/// `unanswered` names it at `check`.
fn settle(family: Option<&str>, layout: Option<&Layout>, boot: &str) -> Option<(Family, String)> {
    let known = family.and_then(self::family);
    let uki = !boot.is_empty();
    let told = |pick: fn(&Layout) -> &str| {
        layout
            .map(pick)
            .filter(|declared| !declared.is_empty())
            .map(str::to_string)
    };
    let bootloader = if boot.is_empty() {
        told(|l| &l.bootloader)?
    } else {
        "systemd".into()
    };
    let family = Family {
        composefs: uki
            || layout
                .and_then(|l| l.composefs)
                .or(known.as_ref().map(|f| f.composefs))?,
        generic: uki
            || layout
                .and_then(|l| l.generic)
                .or(known.as_ref().map(|f| f.generic))?,
        // A UKI image settles its own root: btrfs, the owner's default
        // (NEXT-44, 2026-09-18). A declared layout never reaches here, because
        // the boot chain is the image's own and the family's answer is for
        // images that declare none.
        filesystem: if uki {
            "btrfs".into()
        } else {
            told(|l| &l.filesystem).or_else(|| known.as_ref().map(|f| f.filesystem.clone()))?
        },
        admin: told(|l| &l.admin_group).or_else(|| known.as_ref().map(|f| f.admin.clone()))?,
    };
    Some((family, bootloader))
}

/// Which of the five neither the family nor the layout answers, in the order
/// the schema lists them. Empty where the image is installable. No family
/// answers the bootloader.
pub fn unanswered(family: &str, layout: Option<&Layout>, boot: &str) -> Vec<&'static str> {
    if settle(Some(family), layout, boot).is_some() {
        return Vec::new();
    }
    let known = self::family(family);
    let told = |pick: fn(&Layout) -> &str| layout.is_some_and(|l| !pick(l).is_empty());
    [
        ("filesystem", told(|l| &l.filesystem) || !boot.is_empty()),
        (
            "composefs",
            layout.is_some_and(|l| l.composefs.is_some()) || !boot.is_empty(),
        ),
        (
            "generic-image",
            layout.is_some_and(|l| l.generic.is_some()) || !boot.is_empty(),
        ),
        ("admin-group", told(|l| &l.admin_group)),
        ("bootloader", told(|l| &l.bootloader) || !boot.is_empty()),
    ]
    .into_iter()
    .filter(|(name, _)| known.is_none() || *name == "bootloader")
    .filter(|(_, declared)| !declared)
    .map(|(name, _)| name)
    .collect()
}

/// Why `build` has no recipe for a target, for the command that asked for one.
pub fn refusal(list: &List, name: &str) -> String {
    let declared = list
        .targets()
        .into_iter()
        .find(|t| t.to_string() == name)
        .and_then(|t| list.images.iter().find(|i| i.id == t.image));
    let Some(image) = declared else {
        return format!("`{name}` is not a target here");
    };
    let Some(base) = image.base.as_ref() else {
        return format!("`{name}` declares no base, so there is nothing to install");
    };
    format!(
        "`{name}` answers no {}, and guessing one erases a disk before it fails to \
         boot\n\nhelp: declare it in the image's `layout {{ }}`; the bootloader is one \
         its base's row in bases.kdl lists",
        unanswered(&base.family, image.layout.as_ref(), &image.boot).join(", ")
    )
}

/// Whether this image's install seals the deployment, which is what makes
/// fs-verity a requirement and refuses xfs. Read where an image's own
/// `filesystem` is checked, so the table stays in one place.
pub fn seals(family: &str, layout: Option<&Layout>, boot: &str) -> bool {
    !boot.is_empty()
        || layout
            .and_then(|l| l.composefs)
            .unwrap_or_else(|| self::family(family).is_some_and(|f| f.composefs))
}

/// The recipe for one target: `image` is the bytes installed and `imgref` the
/// reference the installed machine updates from. `stores` are host paths
/// carrying that image offline, empty where it is pulled. `None` when nothing
/// publishes under that name, when the image declares no base, or when
/// `settle` has no answer; `refusal` says which.
pub fn build(
    list: &List,
    name: &str,
    image: &str,
    imgref: &str,
    stores: &[String],
) -> Option<Json> {
    let published = list
        .targets()
        .into_iter()
        .find(|target| target.to_string() == name)?
        .published();
    let (declared, _, entries) = of_target(list, name)?;
    let layout = declared.layout.as_ref();
    let (settled, bootloader) = settle(
        Some(&declared.base.as_ref()?.family),
        layout,
        &declared.boot,
    )?;

    let mut fields = vec![
        ("image", Json::string(image)),
        ("targetImgref", Json::string(imgref)),
        ("composeFsBackend", Json::Bool(settled.composefs)),
        ("genericImage", Json::Bool(settled.generic)),
        ("bootloader", Json::string(&bootloader)),
        ("filesystem", Json::string(&settled.filesystem)),
        // The published name, which is a hostname the person installing is
        // free to replace. Every other field here is one they are not.
        ("hostname", Json::string(published)),
        (
            "user",
            Json::object([("groups", Json::strings([&settled.admin]))]),
        ),
    ];
    if !declared.boot.is_empty() {
        fields.push(("boot", Json::string(&declared.boot)));
    }
    if provides(declared, &entries, LUKS_INITRAMFS) {
        fields.push(("luksInitramfs", Json::Bool(true)));
    }
    // Both are meaningful for one filesystem each, and fisherman ignores the
    // other. Emitted only where declared, so a recipe says what the image said.
    if layout.is_some_and(|l| l.subvolumes) {
        fields.push(("btrfsSubvolumes", Json::Bool(true)));
    }
    if let Some(pool) = layout.map(|l| l.pool.as_str()).filter(|p| !p.is_empty()) {
        fields.push(("zfsPoolName", Json::string(pool)));
    }
    if let Some(var) = layout.and_then(|l| l.var_disk.as_ref()) {
        fields.push((
            "varDisk",
            Json::object([
                ("disk", Json::string(&var.disk)),
                ("keepExisting", Json::Bool(var.keep_existing)),
            ]),
        ));
    }
    if !stores.is_empty() {
        fields.push((
            "additionalImageStores",
            Json::strings(stores.iter().cloned()),
        ));
    }
    Some(Json::object(fields))
}

/// Where the media carries the selected image, and where the live environment
/// registers it as an additional image store. Handed to `bootc install` and
/// written into the live environment's `storage.conf`: fisherman pulls before
/// it starts the install container, and that pull knows nothing of the recipe.
pub const STORE: &str = "/var/lib/tectonic/store";

/// The staging ceiling the media is assembled in, not a cost.
const SIZE: &str = "20G";

/// The live environment the media boots, layered on the image being installed.
pub const LIVE_ENV: &str = include_str!("installer.Containerfile");

/// The one patch this project carries upstream, applied to tacklebox in the
/// live environment's builder stage. It belongs beside the Containerfile that
/// applies it, and both are staged into the same build context.
pub const EFI_PATCH: &str = include_str!("installer-efi-from-image.patch");

/// What the live environment is built and tagged as. Per target, because the
/// media's recipe is baked into it and root's image store is shared: a stale
/// tag there ships silently.
pub fn live(published: &str) -> String {
    format!("localhost/{published}-installer:latest")
}

/// The media the installer is assembled into, as the recipe tacklebox takes.
/// `image` is the local build embedded in the media and `imgref` the name it is
/// embedded under, so the installed machine's update origin is the published one
/// while no byte of the install came from a registry. `None` on the same three
/// refusals `build` makes.
pub fn media(list: &List, name: &str, image: &str, imgref: &str) -> Option<Json> {
    let target = list.targets().into_iter().find(|t| t.to_string() == name)?;
    let declared = list.images.iter().find(|i| i.id == target.image)?;
    settle(
        Some(&declared.base.as_ref()?.family),
        declared.layout.as_ref(),
        &declared.boot,
    )?;
    let published = target.published();

    Some(Json::object([
        ("media_name", Json::string(format!("{published}-install"))),
        ("size", Json::string(SIZE)),
        // `esp_partition` stays unset, so tacklebox keeps its default of off.
        // Appending the ESP as a 0xEF partition costs CD-ROM boot: xorriso
        // records the El Torito EFI image with a load size of 0, because the
        // image is the appended partition and its length is not known when the
        // catalog is written.
        (
            "bootable_environments",
            Json::array([Json::object([
                ("id", Json::string("installer")),
                ("image", Json::string(live(&published))),
                // The image's name and not its `pretty-name`, which is
                // optional and empty in most repositories: this is a boot menu
                // entry, and there is no such thing as a blank one.
                (
                    "title",
                    Json::string(format!("{} installer", declared.name)),
                ),
                ("modes", Json::strings(["live"])),
            ])]),
        ),
        (
            "offline_payloads",
            Json::array([Json::object([
                ("source", Json::string(image)),
                ("ref", Json::string(imgref)),
            ])]),
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> List {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/repos")
            .join(name);
        let (list, issues) = List::load(&root);
        assert!(issues.is_empty(), "{name} is a clean fixture");
        list
    }

    /// The whole of what this decides, on the two families that exist, read off
    /// real declarations.
    #[test]
    fn the_family_settles_the_boot_chain_and_nothing_else_does() {
        let deb = fixture("deb-families");
        let recipe = build(
            &deb,
            "forky",
            "localhost/forky:latest",
            "ghcr.io/someone/forky:latest",
            &[STORE.to_string()],
        )
        .expect("a debian target has an answer");
        assert_eq!(
            recipe.render(),
            format!(
                "{{\n  \"image\": \"localhost/forky:latest\",\n  \"targetImgref\": \
                 \"ghcr.io/someone/forky:latest\",\n  \"composeFsBackend\": true,\n  \
                 \"genericImage\": true,\n  \"bootloader\": \"grub2\",\n  \
                 \"filesystem\": \"ext4\",\n  \"hostname\": \"forky\",\n  \"user\": \
                 {{\n    \"groups\": [\n      \"sudo\"\n    ]\n  }},\n  \
                 \"additionalImageStores\": [\n    \"{STORE}\"\n  ]\n}}\n"
            )
        );

        // An ubuntu image is the same family answer, and a fedora one is the
        // other: a constant wrong in either direction would install a disk that
        // does not boot, with no build failure anywhere to catch it.
        let field =
            |list: &List, name: &str, key: &str| match build(list, name, "image", "imgref", &[]) {
                Some(Json::Object(fields)) => fields
                    .iter()
                    .find(|(had, _)| had == key)
                    .map(|(_, value)| value.render().trim().to_string()),
                _ => None,
            };
        assert_eq!(
            field(&deb, "ubuntu", "bootloader").as_deref(),
            Some("\"grub2\"")
        );
        assert_eq!(field(&deb, "ubuntu", "user"), field(&deb, "forky", "user"));

        let fedora = fixture("minimal");
        let target = fedora.ungated_target().expect("the fixture publishes one");
        let target = target.to_string();
        assert_eq!(
            field(&fedora, &target, "composeFsBackend").as_deref(),
            Some("false")
        );
        assert_eq!(
            field(&fedora, &target, "genericImage").as_deref(),
            Some("false")
        );
        assert_eq!(
            field(&fedora, &target, "filesystem").as_deref(),
            Some("\"xfs\"")
        );
        assert_eq!(
            field(&fedora, &target, "user").as_deref(),
            Some("{\n  \"groups\": [\n    \"wheel\"\n  ]\n}")
        );

        // A store nothing carries is an absent key, because fisherman
        // bind-mounts every path it is given.
        assert!(field(&deb, "forky", "additionalImageStores").is_none());
    }

    /// The image's own answer where it declares one, and nothing else moves.
    /// A recipe carrying the family's ext4 under a declared btrfs installs a
    /// disk the image did not ask for and nothing anywhere would say so.
    #[test]
    fn a_declared_layout_replaces_the_family_filesystem_and_adds_the_var_disk() {
        let deb = fixture("deb-families");
        let recipe = build(&deb, "debian", "image", "imgref", &[]).expect("a debian target");
        let field = |key: &str| match &recipe {
            Json::Object(fields) => fields
                .iter()
                .find(|(had, _)| had == key)
                .map(|(_, value)| value.render().trim().to_string()),
            _ => None,
        };
        assert_eq!(field("filesystem").as_deref(), Some("\"btrfs\""));
        assert_eq!(field("btrfsSubvolumes").as_deref(), Some("true"));
        // The base's row lists grub2 first, so this is the image's own pick.
        assert_eq!(field("bootloader").as_deref(), Some("\"systemd\""));
        assert_eq!(
            field("varDisk").as_deref(),
            Some("{\n  \"disk\": \"/dev/sdb\",\n  \"keepExisting\": false\n}")
        );
        // The family still settles everything the layout does not name.
        assert_eq!(field("composeFsBackend").as_deref(), Some("true"));

        // A target whose image declares no layout carries none of the four
        // optional fields: fisherman formats every disk it is handed, and its
        // own defaults are what an absent key means.
        let plain = build(&deb, "forky", "image", "imgref", &[]).expect("a debian target");
        let Json::Object(fields) = plain else {
            panic!("a recipe is an object")
        };
        for absent in ["varDisk", "btrfsSubvolumes", "zfsPoolName"] {
            assert!(fields.iter().all(|(had, _)| had != absent), "{absent}");
        }
        assert!(fields
            .iter()
            .any(|(had, value)| had == "bootloader" && value.render().trim() == "\"grub2\""));
    }

    /// The one pair the family refuses, checked where the table lives, and an
    /// image saying so for itself on a family nothing here has measured.
    #[test]
    fn a_sealed_family_is_the_only_one_that_refuses_a_filesystem() {
        assert!(seals("debian", None, ""));
        assert!(seals("ubuntu", None, ""));
        assert!(!seals("fedora", None, ""));
        assert!(!seals("plan9", None, ""));

        let sealing = layout(Some(true));
        let plain = layout(Some(false));
        assert!(seals("plan9", Some(&sealing), ""));
        assert!(seals("fedora", Some(&sealing), ""), "the image overrides");
        assert!(!seals("debian", Some(&plain), ""), "in both directions");
    }

    fn layout(composefs: Option<bool>) -> crate::model::image::Layout {
        crate::model::image::Layout {
            filesystem: String::new(),
            subvolumes: false,
            pool: String::new(),
            bootloader: String::new(),
            composefs,
            generic: None,
            admin_group: String::new(),
            var_disk: None,
            span: crate::diag::Span::default(),
        }
    }

    /// EL installs the way Fedora does, measured 2026-09-10 by installing a
    /// CentOS Stream 10 image from `tect` media. What its kernel has not got is
    /// btrfs.
    #[test]
    fn rhel_installs_the_way_fedora_does() {
        let mut grub = layout(None);
        grub.bootloader = "grub2".into();
        assert!(settle(Some("rhel"), Some(&grub), "").is_some());
        assert_eq!(
            settle(Some("rhel"), Some(&grub), ""),
            settle(Some("fedora"), Some(&grub), "")
        );
        assert_eq!(unanswered("rhel", Some(&grub), ""), Vec::<&str>::new());
        assert!(!seals("rhel", None, ""));
    }

    /// No family answers the bootloader: an image that names none has no
    /// recipe, and the refusal says which answer is missing.
    #[test]
    fn the_bootloader_is_the_image_s_own_answer() {
        assert_eq!(unanswered("fedora", None, ""), ["bootloader"]);
        assert_eq!(settle(Some("debian"), None, ""), None);
        let deb = fixture("deb-families");
        assert!(build(&deb, "ubuntu", "image", "imgref", &[]).is_some());

        let silent = fixture("multi-image");
        let name = silent.ungated_target().expect("a target").to_string();
        assert!(build(&silent, &name, "image", "imgref", &[]).is_none());
        assert!(refusal(&silent, &name).contains("answers no bootloader"));
    }

    /// A family with no measured answer is installable once the image gives
    /// all five, and the diagnostic names what half an answer is missing.
    #[test]
    fn an_unmeasured_family_answers_for_itself_in_full_or_not_at_all() {
        assert_eq!(
            unanswered("plan9", None, ""),
            [
                "filesystem",
                "composefs",
                "generic-image",
                "admin-group",
                "bootloader"
            ]
        );

        let mut half = layout(Some(true));
        half.filesystem = "btrfs".into();
        assert_eq!(
            unanswered("plan9", Some(&half), ""),
            ["generic-image", "admin-group", "bootloader"]
        );
        // A family the tool knows fills every gap but the bootloader, so a
        // partial layout there is an override rather than a hole.
        assert_eq!(unanswered("fedora", Some(&half), ""), ["bootloader"]);
        assert_eq!(unanswered("plan9", None, "uki-db"), ["admin-group"]);

        let mut whole = half;
        whole.generic = Some(true);
        whole.admin_group = "wheel".into();
        whole.bootloader = "systemd".into();
        assert_eq!(unanswered("plan9", Some(&whole), ""), Vec::<&str>::new());
        let (settled, bootloader) =
            settle(Some("plan9"), Some(&whole), "").expect("all five are declared");
        assert_eq!(bootloader, "systemd");
        assert_eq!(settled.filesystem, "btrfs");
        assert_eq!(settled.admin, "wheel");
        assert!(settled.composefs && settled.generic);
    }

    /// The whole of the general case, read off a real declaration: a family
    /// nothing here has measured produces a recipe once the image answers all
    /// five, and every field in it is the image's own.
    #[test]
    fn an_image_on_an_unmeasured_family_installs_off_its_own_declaration() {
        let arch = fixture("unmeasured-family");
        let recipe = build(&arch, "arch", "localhost/arch:latest", "imgref", &[])
            .expect("all five are declared");
        assert_eq!(
            recipe.render(),
            "{\n  \"image\": \"localhost/arch:latest\",\n  \"targetImgref\": \"imgref\",\n  \
             \"composeFsBackend\": true,\n  \"genericImage\": true,\n  \"bootloader\": \
             \"systemd\",\n  \"filesystem\": \"btrfs\",\n  \"hostname\": \"arch\",\n  \
             \"user\": {\n    \"groups\": [\n      \"wheel\"\n    ]\n  },\n  \
             \"btrfsSubvolumes\": true\n}\n"
        );
        // Both documents on one medium, or the media is assembled around a
        // target no installer on it can install.
        assert!(media(&arch, "arch", "localhost/arch:latest", "imgref").is_some());
    }

    /// The media carries the local bytes under the published name, which is
    /// what makes the install offline and the update origin right at once.
    #[test]
    fn the_media_embeds_the_local_build_under_the_published_name() {
        let deb = fixture("deb-families");
        let assembled = media(
            &deb,
            "forky",
            "localhost/forky:latest",
            "ghcr.io/someone/forky:latest",
        )
        .expect("a debian target has an answer");
        assert_eq!(
            assembled.render(),
            "{\n  \"media_name\": \"forky-install\",\n  \"size\": \"20G\",\n  \
             \"bootable_environments\": [\n    {\n      \"id\": \"installer\",\n      \
             \"image\": \"localhost/forky-installer:latest\",\n      \"title\": \
             \"Forky installer\",\n      \"modes\": [\n        \"live\"\n      ]\n    }\n  ],\n  \"offline_payloads\": [\n    {\n      \"source\": \
             \"localhost/forky:latest\",\n      \"ref\": \
             \"ghcr.io/someone/forky:latest\"\n    }\n  ]\n}\n"
        );

        // Both documents on one medium refuse together, or the media would be
        // assembled around a target no installer on it can install.
        assert!(media(&deb, "not-a-target", "image", "imgref").is_none());
    }

    /// Both installer units hand over to the binary this file stages, and
    /// they hand over to no other.
    ///
    /// **This proves less than the test it replaces, and the difference
    /// matters to a later reader.** Until `NEXT-53` stage 3 the units ran
    /// `/usr/bin/tect installer`, and the tie was that the word `installer`
    /// resolved through `crate::command::resolve`. The installer is another
    /// crate's binary now, so no command table here can answer for it and that
    /// tie is gone rather than moved. What survives is that each unit's
    /// `ExecStart=` ends at the path the `COPY` above writes. A binary that
    /// cannot run is not caught here, and the Containerfile's own `--version`
    /// is what catches that.
    #[test]
    fn both_installer_units_hand_over_to_the_binary_the_live_environment_stages() {
        // Source and destination both. A `COPY` checked by its destination
        // alone still passes when the source becomes another binary, and
        // fisherman would then draw on tty1.
        const STAGES: &str = "COPY --from=tools /out/tect-installer /usr/bin/tect-installer";
        const RUNS: &str = "/usr/bin/tect-installer";
        assert!(
            LIVE_ENV.contains(STAGES),
            "the live environment does not stage the installer"
        );
        // Each unit is read through the file it writes, rather than by
        // filtering every `ExecStart=` in this Containerfile for one that
        // looks like the installer. A filter loses the line that stopped
        // naming the installer, and losing it is the edit that leaves the
        // media booting to a login prompt, a root shell or a respawn loop.
        for unit in ["tect-installer.service", "tect-installer-vt.service"] {
            let heredoc = format!("/usr/lib/systemd/system/{unit}\n");
            let body = LIVE_ENV
                .split_once(&heredoc)
                .map(|(_, rest)| rest.split_once("\nUNIT\n").map_or(rest, |(body, _)| body))
                .unwrap_or_else(|| panic!("{unit} is never written"));
            let exec: Vec<&str> = body
                .lines()
                .filter_map(|line| line.strip_prefix("ExecStart="))
                .collect();
            assert_eq!(exec.len(), 1, "{unit} runs {} things", exec.len());
            // The last word, because one unit runs the installer directly and
            // the other hands it to kmscon as that command's own argument. A
            // `-` or `@` prefix would fail here, and it should: this unit
            // failing is what the other one exists to catch.
            assert_eq!(
                exec[0].split_whitespace().last(),
                Some(RUNS),
                "{unit} hands over to something else: {}",
                exec[0]
            );
        }
    }

    /// What a resolving verb never proved: that a unit is enabled at all, that
    /// it draws on the console the media shows a person, and that no login is
    /// left on that console beside it. Media can fail every one of these while
    /// the invocation above is perfectly good, which is why they are not one
    /// test any more.
    #[test]
    fn the_units_that_autostart_the_installer_own_the_console_they_draw_on() {
        // Every unit that runs it is enabled by name somewhere, or the media
        // boots to whatever else claims the console.
        for unit in ["tect-installer.service", "tect-installer-vt.service"] {
            assert!(
                LIVE_ENV.contains(&format!("systemctl enable {unit}")),
                "{unit} is never enabled"
            );
            assert!(
                LIVE_ENV.contains(&format!("/usr/lib/systemd/system/{unit}")),
                "{unit} is enabled but never written"
            );
        }
        // Directive lines, not mentions: the comments above these units name
        // the same settings, and a test counting prose would pass on prose.
        let directives = |want: &str| LIVE_ENV.lines().filter(|line| line.trim() == want).count();
        // tty1 is the console the media shows, and `getty@tty1` is the login
        // that must not be on it. One of each per unit, or a switch to that VT
        // hands it to a login and stops the installer for the rest of the boot.
        assert_eq!(directives("Conflicts=getty@tty1.service"), 2);
        // Masked by name, both of them: `autovt@tty1` is its own unit, and
        // masking the getty does not cover the name logind actually starts.
        let masks = LIVE_ENV
            .lines()
            .find(|line| line.contains("systemctl mask getty@"))
            .expect("the getty on the installer's console is masked");
        for masked in ["getty@tty1.service", "autovt@tty1.service"] {
            assert!(masks.contains(masked), "{masked} is not in {masks}");
        }
        assert_eq!(directives("TTYPath=/dev/tty1"), 1);
        // kmscon respawns its child in place without `--oneshot`, so the
        // installer exiting would be invisible to systemd, `Restart=` would
        // never run and the fallback would be unreachable.
        let kmscon: Vec<&str> = LIVE_ENV
            .lines()
            .filter(|line| line.starts_with("ExecStart=") && line.contains("kmscon"))
            .collect();
        assert_eq!(kmscon.len(), 1, "{kmscon:?}");
        for want in ["--vt=1", "--oneshot", "--no-switchvt"] {
            assert!(kmscon[0].contains(want), "{want} is not in {}", kmscon[0]);
        }
    }

    /// No family, no recipe. The refusal is the point: `bootc install` reaches
    /// a bootloader minutes after the disk is wiped.
    #[test]
    fn a_family_with_no_measured_answer_is_refused_rather_than_defaulted() {
        assert!(settle(Some("plan9"), None, "").is_none());
        assert!(settle(Some(""), None, "").is_none());
        // Half an answer is refused the same way, and `check` names it.
        let mut half = layout(Some(true));
        half.filesystem = "btrfs".into();
        assert!(settle(Some("plan9"), Some(&half), "").is_none());
        let deb = fixture("deb-families");
        assert!(build(&deb, "not-a-target", "image", "imgref", &[]).is_none());
    }

    #[test]
    fn a_uki_chain_sets_systemd_boot_without_a_layout_answer() {
        let (family, bootloader) = settle(Some("fedora"), None, "uki-db").expect("a known family");
        assert_eq!(bootloader, "systemd");
        assert!(family.composefs);
        assert!(family.generic);
        // The UKI path settles btrfs (NEXT-44, 2026-09-18): the owner's
        // default root, where the old one was the family's ext4.
        assert_eq!(family.filesystem, "btrfs");
        assert!(seals("fedora", None, "uki-db"));
        assert_eq!(unanswered("fedora", None, "uki-db"), Vec::<&str>::new());

        let list = fixture("uki");
        let recipe = build(&list, "shim", "image", "imgref", &[])
            .expect("an installable UKI image")
            .render();
        assert!(recipe.contains("\"bootloader\": \"systemd\""), "{recipe}");
        assert!(recipe.contains("\"boot\": \"uki-shim\""), "{recipe}");
        assert!(recipe.contains("\"luksInitramfs\": true"), "{recipe}");
    }
}
