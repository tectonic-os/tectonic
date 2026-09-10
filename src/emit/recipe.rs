//! The installation recipe one target installs from.
//!
//! An installer asks a person for the disk, the account and the encryption.
//! Everything else is a property of the image and is derived from the
//! declaration here: a wrong answer erases a disk that then does not boot.

use crate::emit::json::Json;
use crate::model::image::{Layout, List};
use crate::resolve::workflow::FEDORA;

/// What the base family settles: three `bootc install` flags, the root
/// filesystem the first of them forces, and the group an administrator is
/// created in.
struct Family {
    composefs: bool,
    generic: bool,
    /// Fisherman reads an empty bootloader as grub2, which is what bootupd
    /// installs.
    bootloader: String,
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
        // Fedora ships bootupd, so `--generic-image` — which skips the bootupd
        // check `bootc install` aborts on — stays off, the boot chain is the
        // grub2 bootupd installs, and nothing seals the deployment.
        FEDORA => Family {
            composefs: false,
            generic: false,
            bootloader: String::new(),
            filesystem: "xfs".into(),
            admin: "wheel".into(),
        },
        // Debian packages no bootupd, so the install aborts without
        // `--generic-image`. The sealed composefs deployment needs fs-verity:
        // xfs has none and drops into a dracut emergency shell, and sealed
        // btrfs fails to mount. `grub2` because the base stages Debian's signed
        // shim and GRUB, so the disk boots with Secure Boot on; that GRUB reads
        // no BLS entries, so the image ships `/usr/libexec/grub-menu-from-bls`
        // and the installer runs it, and without it the menu comes up empty.
        "debian" | "ubuntu" => Family {
            composefs: true,
            generic: true,
            bootloader: "grub2".into(),
            filesystem: "ext4".into(),
            admin: "sudo".into(),
        },
        _ => return None,
    })
}

/// The five settled, from the family that knows them or from the image that
/// declared them. `None` when neither answers, which is what stops a guess
/// reaching a partition table.
///
/// **Declaring is not guessing.** A family this project has measured answers
/// for an image that declares nothing. An image on any other base answers for
/// itself, in full or not at all: half an answer is the one state that would
/// reach `bootc install` on a default nobody chose, and `unanswered` names it
/// at `check`.
fn settle(family: Option<&str>, layout: Option<&Layout>) -> Option<Family> {
    let known = family.and_then(self::family);
    let told = |pick: fn(&Layout) -> &str| {
        layout
            .map(pick)
            .filter(|declared| !declared.is_empty())
            .map(str::to_string)
    };
    Some(Family {
        composefs: layout
            .and_then(|l| l.composefs)
            .or(known.as_ref().map(|f| f.composefs))?,
        generic: layout
            .and_then(|l| l.generic)
            .or(known.as_ref().map(|f| f.generic))?,
        bootloader: told(|l| &l.bootloader)
            .or_else(|| known.as_ref().map(|f| f.bootloader.clone()))?,
        filesystem: told(|l| &l.filesystem)
            .or_else(|| known.as_ref().map(|f| f.filesystem.clone()))?,
        admin: told(|l| &l.admin_group).or_else(|| known.as_ref().map(|f| f.admin.clone()))?,
    })
}

/// Which of the five neither the family nor the layout answers, in the order
/// the schema lists them. Empty where the image is installable.
pub fn unanswered(family: &str, layout: Option<&Layout>) -> Vec<&'static str> {
    if settle(Some(family), layout).is_some() {
        return Vec::new();
    }
    let known = self::family(family);
    let told = |pick: fn(&Layout) -> &str| layout.is_some_and(|l| !pick(l).is_empty());
    [
        ("filesystem", told(|l| &l.filesystem)),
        ("composefs", layout.is_some_and(|l| l.composefs.is_some())),
        ("generic-image", layout.is_some_and(|l| l.generic.is_some())),
        ("admin-group", told(|l| &l.admin_group)),
        ("bootloader", told(|l| &l.bootloader)),
    ]
    .into_iter()
    .filter(|_| known.is_none())
    .filter(|(_, declared)| !declared)
    .map(|(name, _)| name)
    .collect()
}

/// Whether this image's install seals the deployment, which is what makes
/// fs-verity a requirement and refuses xfs. Read where an image's own
/// `filesystem` is checked, so the table stays in one place.
pub fn seals(family: &str, layout: Option<&Layout>) -> bool {
    layout
        .and_then(|l| l.composefs)
        .unwrap_or_else(|| self::family(family).is_some_and(|f| f.composefs))
}

/// The recipe for one target: `image` is the bytes installed and `imgref` the
/// reference the installed machine updates from. `stores` are host paths
/// carrying that image offline, empty where it is pulled. `None` when nothing
/// publishes under that name, when the image declares no base, or when the
/// family has no answer above.
pub fn build(
    list: &List,
    name: &str,
    image: &str,
    imgref: &str,
    stores: &[String],
) -> Option<Json> {
    let target = list.targets().into_iter().find(|t| t.to_string() == name)?;
    let declared = list.images.iter().find(|i| i.id == target.image)?;
    let layout = declared.layout.as_ref();
    let settled = settle(Some(&declared.base.as_ref()?.family), layout)?;

    let mut fields = vec![
        ("image", Json::string(image)),
        ("targetImgref", Json::string(imgref)),
        ("composeFsBackend", Json::Bool(settled.composefs)),
        ("genericImage", Json::Bool(settled.generic)),
        ("bootloader", Json::string(&settled.bootloader)),
        ("filesystem", Json::string(&settled.filesystem)),
        // The published name, which is a hostname the person installing is
        // free to replace. Every other field here is one they are not.
        ("hostname", Json::string(target.published())),
        (
            "user",
            Json::object([("groups", Json::strings([&settled.admin]))]),
        ),
    ];
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
        // The family's own answer here is grub2, so this is the override
        // landing rather than the default agreeing with it.
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
        assert!(seals("debian", None));
        assert!(seals("ubuntu", None));
        assert!(!seals(FEDORA, None));
        assert!(!seals("plan9", None));

        let sealing = layout(Some(true));
        let plain = layout(Some(false));
        assert!(seals("plan9", Some(&sealing)));
        assert!(seals(FEDORA, Some(&sealing)), "the image overrides");
        assert!(!seals("debian", Some(&plain)), "in both directions");
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

    /// A family with no measured answer is installable once the image gives
    /// all five, and the diagnostic names what half an answer is missing.
    #[test]
    fn an_unmeasured_family_answers_for_itself_in_full_or_not_at_all() {
        assert_eq!(unanswered(FEDORA, None), Vec::<&str>::new());
        assert_eq!(
            unanswered("plan9", None),
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
            unanswered("plan9", Some(&half)),
            ["generic-image", "admin-group", "bootloader"]
        );
        // A family the tool knows fills every gap, so a partial layout there is
        // an override rather than a hole.
        assert_eq!(unanswered(FEDORA, Some(&half)), Vec::<&str>::new());

        let mut whole = half;
        whole.generic = Some(true);
        whole.admin_group = "wheel".into();
        whole.bootloader = "systemd".into();
        assert_eq!(unanswered("plan9", Some(&whole)), Vec::<&str>::new());
        let settled = settle(Some("plan9"), Some(&whole)).expect("all five are declared");
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

    /// The live environment starts the installer by typing its name, and
    /// nothing else ties that word to the command table.
    #[test]
    fn the_verb_the_live_environment_autostarts_is_one_that_resolves() {
        let typed: Vec<&str> = LIVE_ENV
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("tect "))
            .expect("the live environment types one command")
            .split_whitespace()
            .skip(1)
            .collect();
        let resolved = crate::command::resolve(&typed);
        assert!(resolved.is_ok(), "{typed:?}: {:?}", resolved.err());
    }

    /// No family, no recipe. The refusal is the point: `bootc install` reaches
    /// a bootloader minutes after the disk is wiped.
    #[test]
    fn a_family_with_no_measured_answer_is_refused_rather_than_defaulted() {
        assert!(settle(Some("plan9"), None).is_none());
        assert!(settle(Some(""), None).is_none());
        // Half an answer is refused the same way, and `check` names it.
        let mut half = layout(Some(true));
        half.filesystem = "btrfs".into();
        assert!(settle(Some("plan9"), Some(&half)).is_none());
        let deb = fixture("deb-families");
        assert!(build(&deb, "not-a-target", "image", "imgref", &[]).is_none());
    }
}
