//! The installation recipe one target installs from.
//!
//! An installer asks a person for the disk, the account and the encryption.
//! Everything else in a recipe is a property of the image, and a wrong answer
//! to one of those is a disk that is erased and then does not boot — so they
//! are derived from the declaration here rather than offered as questions.

use crate::emit::json::Json;
use crate::model::image::List;
use crate::resolve::workflow::FEDORA;

/// What the base family settles: three `bootc install` flags, the root
/// filesystem the first of them forces, and the group an administrator is
/// created in.
struct Family {
    composefs: bool,
    generic: bool,
    /// Fisherman reads an empty bootloader as grub2, which is what bootupd
    /// installs.
    bootloader: &'static str,
    filesystem: &'static str,
    /// Naming the other family's group as well is not a hedge that covers
    /// both: `useradd` refuses the whole call when any listed group is
    /// missing, so a two-group list fails on every target.
    admin: &'static str,
}

/// `None` for a family with no answer here, which is what stops a guess
/// reaching a partition table.
fn family(name: &str) -> Option<Family> {
    Some(match name {
        // Fedora ships bootupd, so `--generic-image` — which exists to skip
        // the bootupd check `bootc install` otherwise aborts on — stays off,
        // the boot chain is the grub2 bootupd installs, and nothing seals the
        // deployment. Not measured here: this is the path fisherman documents
        // for the images that do carry bootupd, and no tect image has been
        // installed through it yet.
        FEDORA => Family {
            composefs: false,
            generic: false,
            bootloader: "",
            filesystem: "xfs",
            admin: "wheel",
        },
        // Debian packages no bootupd at all, so the install aborts without
        // `--generic-image` and reaches systemd-boot instead. This project's
        // own base seals the deployment with composefs, and a sealed one needs
        // fs-verity: xfs has none and drops into a dracut emergency shell,
        // and a sealed btrfs deployment fails to mount. Measured `NEXT-40`.
        "debian" | "ubuntu" => Family {
            composefs: true,
            generic: true,
            bootloader: "systemd",
            filesystem: "ext4",
            admin: "sudo",
        },
        _ => return None,
    })
}

/// The recipe for one target: `image` is the bytes installed and `imgref` the
/// reference the installed machine updates from, which is the whole of what
/// keeps a local build off a machine's update origin. `stores` are host paths
/// carrying that image offline, empty where it is pulled.
///
/// `None` when nothing publishes under that name, when the image declares no
/// base, or when the family has no answer above.
pub fn build(
    list: &List,
    name: &str,
    image: &str,
    imgref: &str,
    stores: &[String],
) -> Option<Json> {
    let target = list.targets().into_iter().find(|t| t.to_string() == name)?;
    let declared = list.images.iter().find(|i| i.id == target.image)?;
    let family = self::family(&declared.base.as_ref()?.family)?;

    let mut fields = vec![
        ("image", Json::string(image)),
        ("targetImgref", Json::string(imgref)),
        ("composeFsBackend", Json::Bool(family.composefs)),
        ("genericImage", Json::Bool(family.generic)),
        ("bootloader", Json::string(family.bootloader)),
        ("filesystem", Json::string(family.filesystem)),
        // The published name, which is a hostname the person installing is
        // free to replace. Every other field here is one they are not.
        ("hostname", Json::string(target.published())),
        (
            "user",
            Json::object([("groups", Json::strings([family.admin]))]),
        ),
    ];
    if !stores.is_empty() {
        fields.push((
            "additionalImageStores",
            Json::strings(stores.iter().cloned()),
        ));
    }
    Some(Json::object(fields))
}

/// Where the media carries the selected image, and where the live environment
/// registers it as an additional image store. `bootc install` is handed this
/// path, and so is the live environment's own `storage.conf`: fisherman pulls
/// before it starts the install container, and that pull knows nothing about
/// the recipe.
pub const STORE: &str = "/var/lib/tectonic/store";

/// The staging ceiling the media is assembled in, not a cost: the ISO stage 1
/// measured off a 20G recipe was 518 MB.
const SIZE: &str = "20G";

/// What the live environment is built and tagged as. Per target rather than
/// one name for every repository, because the media's recipe is baked into it
/// and root's image store is shared: a stale tag there ships silently.
pub fn live(published: &str) -> String {
    format!("localhost/{published}-installer:latest")
}

/// The media the installer is assembled into, as the recipe tacklebox takes.
///
/// `image` is the local build embedded in the media and `imgref` the name it
/// is embedded *under*, so the installed machine's update origin is the
/// published one while no byte of the install came from a registry. That pair
/// is the whole of the split, and it is one recipe field rather than a design.
///
/// `None` on the same three refusals `build` makes, so the two documents on
/// one medium cannot disagree about whether the target is installable.
pub fn media(list: &List, name: &str, image: &str, imgref: &str) -> Option<Json> {
    let target = list.targets().into_iter().find(|t| t.to_string() == name)?;
    let declared = list.images.iter().find(|i| i.id == target.image)?;
    self::family(&declared.base.as_ref()?.family)?;
    let published = target.published();

    Some(Json::object([
        ("media_name", Json::string(format!("{published}-install"))),
        ("size", Json::string(SIZE)),
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

    /// The whole of what this decides, on the two families that exist, read
    /// off real declarations rather than a constructed one.
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
                 \"genericImage\": true,\n  \"bootloader\": \"systemd\",\n  \
                 \"filesystem\": \"ext4\",\n  \"hostname\": \"forky\",\n  \"user\": \
                 {{\n    \"groups\": [\n      \"sudo\"\n    ]\n  }},\n  \
                 \"additionalImageStores\": [\n    \"{STORE}\"\n  ]\n}}\n"
            )
        );

        // An ubuntu image is the same family answer, and a fedora one is the
        // other: a constant wrong in either direction would install a disk
        // that does not boot rather than fail a build.
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
            Some("\"systemd\"")
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

        // A store nothing carries is an absent key rather than an empty list,
        // because fisherman bind-mounts every path it is given.
        assert!(field(&deb, "forky", "additionalImageStores").is_none());
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

    /// No family, no recipe. The refusal is the point: `bootc install` reaches
    /// a bootloader minutes after the disk is wiped.
    #[test]
    fn a_family_with_no_measured_answer_is_refused_rather_than_defaulted() {
        assert!(self::family("plan9").is_none());
        assert!(self::family("").is_none());
        let deb = fixture("deb-families");
        assert!(build(&deb, "not-a-target", "image", "imgref", &[]).is_none());
    }
}
