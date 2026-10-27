//! manifest.kdl: the component author's file.

use crate::diag::{Issue, Issues, Source, Span};
use crate::layout;
use crate::model::component::{
    Access, BaseFacts, Collect, Component, Contribution, Copr, Coverage, Decl, FileMode, Import,
    Key, Located, PackageGroup, Position, VerifyException,
};
use crate::model::image::{Entry, Image, List};
use crate::model::options::{Opt, OptType};
use crate::parse::disk::Disk;
use crate::parse::prop_span;
use crate::parse::schema::{check_doc, Arg, Kind, Node, Prop, Say, FILE_SCHEMA_VERSION};
use crate::parse::{asset, boolean, check_capability, check_path, child, flag, int_prop, kids};
use crate::parse::{names, options, prop, text};
use crate::parse::{string_arg, string_args, syntax_issue};
use crate::resolve::options as resolve_options;
use crate::runtime::{class_names, VERIFY_CLASSES};
use kdl::{KdlDocument, KdlNode};
use std::collections::BTreeSet;
use std::path::Path;

/// The base families this repository knows how to build on. A family is a
/// package archive: `rhel` and `fedora` share dnf and differ in what it finds.
pub const FAMILIES: [&str; 4] = ["fedora", "rhel", "debian", "ubuntu"];

/// The families dnf installs on, which share the adapter, the repo guard and
/// an SELinux policy.
pub fn rpm(family: &str) -> bool {
    matches!(family, "fedora" | "rhel")
}

const TOKEN_HELP: &str = "package names and repo IDs are emitted straight into the RUN line, so they are limited to letters, digits and . _ + : -, and a package name takes one = before its version; anything else belongs in component.sh, where it can be quoted deliberately";

/// Why a package name or repo ID is not safe to emit, or None when it is.
fn bad_token(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return Some("is empty");
    }
    if value.starts_with('-') {
        return Some("starts with a dash");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._+:-".contains(c))
    {
        return Some("has a character that is not allowed");
    }
    None
}

/// Why a package argument is not safe to emit, or None when it is. One `=`
/// separates the package name from its version, and the emitted line spells
/// the separator the way the family's package manager reads it.
pub(crate) fn bad_package(value: &str) -> Option<&'static str> {
    match value.split_once('=') {
        Some((_, "")) => Some("has no version after its `=`"),
        Some((name, version)) => bad_token(name).or_else(|| bad_token(version)),
        None => bad_token(value),
    }
}

/// The capability that makes a component a base.
pub const BASE_IMAGE: &str = "base-image";

/// The generators the tool implements. A manifest picks one; it never names a
/// command of its own.
const GENERATORS: [&str; 3] = ["cosign", "openssl", "ssh-keygen"];

/// The RSA size an `openssl` key is generated at when the declaration names
/// none.
const BITS: u32 = 4096;

const KEY_SIZES: (i128, i128) = (2048, 16384);

const PRIORITIES: (i128, i128) = (0, 9999);

const NEEDED: Say = Say::new("a `key` needs `{}`", "incomplete", "");

const PRIORITY: Say = Say::new(
    "`priority` is a number from 0 to 9999",
    "not a priority",
    "it becomes the NNNN in the staged filename, so four digits is the whole range there is",
);

#[rustfmt::skip]
const KEY: Node = Node::new("key",
    "A key that `tect create key` generates for this component.").about("A component that needs a key declares it here, such as a Secure Boot signing key. `tect create key` generates the key from this declaration. The build installs the public half into the image at the path that `public` names. The private half stays in `keys/private/`, outside the image and outside git.").example("\"secureboot\"")
    .arg(Arg::Str, Say::new("`key` needs a kind", "no kind given",
        "`key \"cosign\" { ... }`, the kind being what `tect create key` names"))
    .unique(Say::new("key `{}` is declared twice", "already declared above", ""))
    .props(&[], Say::new("unknown key property `{}`", "not part of the schema",
        "a key carries its kind, and everything else as child nodes"))
    .children(&[
        Node::new("generator", "The generator that writes this key.").about("`tect create key` offers every key that a component declares. `generator` names the tool that writes this key, and its properties set the key up for what the component uses it for.").example("\"openssl\" profile=\"module-signing\"")
            .arg(Arg::One(&GENERATORS), Say::new("`{}` is not a generator the tool has",
                "not a generator",
                "the generators are `cosign`, `openssl` and `ssh-keygen`; a manifest picks one \
                 of them by name"))
            .once("")
            .missing(NEEDED)
            .props(&[
                Prop { name: "profile", kind: Kind::One(&["module-signing", "pcr-signing", "tls-ca"]),
                    desc: "The purpose that the generator serves, if the generator has more than one purpose.",
                    say: Say::new("`profile` must be \"module-signing\", \"pcr-signing\" or \"tls-ca\"", "not a profile", ""),
                    missing: Say::NONE },
                Prop { name: "bits", kind: Kind::Int(KEY_SIZES.0, KEY_SIZES.1),
                    desc: "The RSA key size, which is 4096 by default.",
                    say: Say::new("`bits` is a number from 2048 to 16384", "not a key size", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `generator` property `{}`", "not part of the schema",
                "`generator` accepts `profile` and `bits`")),
        Node::new("public",
            "Where the public half ships, which witnesses the `<kind>-key` capability of this \
             component.").about("Where the build installs the public half in the image. The file also witnesses the `<kind>-key` capability, so another component can `requires` the key.").example("\"/usr/share/secureboot/sb_cert.der\" format=\"der\"")
            .arg(Arg::Str, Say::new("`public` needs an absolute path", "no path given", ""))
            .once("")
            .missing(NEEDED)
            .props(&[
                Prop { name: "format", kind: Kind::One(&["pem", "der"]),
                    desc: "The format of the public half, which is PEM by default.",
                    say: Say::new("`format` must be \"pem\" or \"der\"", "not a format", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `public` property `{}`", "not part of the schema",
                "`public` accepts `format`")),
        Node::new("private", "The filename that the private half takes under `keys/private/`.").about("The file name of the private half under `keys/private/`, which git ignores. The build never copies the private half into the image.").example("\"MOK.priv\"")
            .arg(Arg::Str, Say::new("`private` needs a filename", "no filename given", ""))
            .once("")
            .missing(NEEDED),
    ], Say::new("unknown node `{}` in a key", "not part of the schema",
        "a key holds `generator`, `public` and `private`"));

/// The nodes an `on` block may hold, which `COMPONENT` holds too: outside a
/// gate is every family the component targets, inside one is the families it
/// names. Each is a const of its own because `COMPONENT` is a `const Node` whose
/// `.children(&[..])` cannot name the list it is itself part of. Everything
/// absent here is family-neutral.
const GATED: &[Node] = &[
    PACKAGES,
    PACKAGE_GROUPS,
    COPR,
    REQUIRES,
    IMPORTS,
    AFTER,
    SATISFIES,
];

/// Lists the properties of one provided name. A one-name line or the name's
/// own line in a block carries them.
#[rustfmt::skip]
const PROVIDES_PROPS: &[Prop] = &[
    Prop { name: "witness", kind: Kind::Str,
        desc: "The absolute path in the finished image that witnesses the capability. \
            Without it, nothing witnesses the capability.",
        say: Say::new("`witness` must be a string", "not a path", ""),
        missing: Say::NONE },
    Prop { name: "build-only", kind: Kind::Bool,
        desc: "Whether the `witness` exists only while the build runs.",
        say: Say::new("`build-only` takes #true or #false", "not a boolean", ""),
        missing: Say::NONE },
    Prop { name: "exclusive", kind: Kind::Bool,
        desc: "Whether a second enabled provider of the capability conflicts with this one.",
        say: Say::new("`exclusive` takes #true or #false", "not a boolean", ""),
        missing: Say::NONE },
];

#[rustfmt::skip]
const PROVIDED: Node = Node::new("",
    "One capability in a `provides` block, which the node name names.").example("\"ssh\" witness=\"/usr/sbin/sshd\"")
    .arg(Arg::None, Say::new("`{}` is a second name on one line of the block", "one name per line",
        "give each name its own line"))
    .props(PROVIDES_PROPS, Say::new("unknown `provides` property `{}`", "not part of the schema",
        "a provided name accepts `witness`, `build-only` and `exclusive`"))
    .children(&[], Say::new("a provided name holds no `{}`", "not part of the schema", ""));

/// Matches one capability name in the block of a node whose names carry no
/// properties.
#[rustfmt::skip]
const CAPABILITY: Node = Node::new("",
    "One capability in the block, which the node name names.").example("\"kernel-devel\"")
    .arg(Arg::None, Say::new("`{}` is a second name on one line of the block", "one name per line",
        "give each name its own line"))
    .props(&[], Say::new("`{}` is not a property of a capability name", "not part of the schema",
        "a capability name in this block takes no properties"))
    .children(&[], Say::new("a capability name holds no `{}`", "not part of the schema", ""));

#[rustfmt::skip]
const REQUIRES: Node = Node::new("requires",
    "A capability that another component must provide, which also orders the build.").about("Names a capability that the image must have for this component to work. A component meets it by its own path, or by a `provides` name equal to it or below it, so `rpm/dnf5` meets `rpm`. The base, a component that the image lists or a component that an `imports` brings must meet it. If none does, then `tect check` fails, and `tect import component` offers the providers. `tect` builds this component after each component that meets it.").example("\"kernel-devel\"")
    .arg(Arg::Names, Say::new("`{}` needs a capability name", "nothing named", ""))
    .props(&[], Say::new("`{}` is not a `requires` property", "not part of the schema",
        "`requires` takes capability names alone; a component that chooses its provider \
         imports it, as `imports \"$container-runtime\"`"))
    .children(&[CAPABILITY], Say::NONE);

#[rustfmt::skip]
const IMPORTS: Node = Node::new("imports",
    "A component that builds in every image that holds this component.").about("Names a component under `components/` by its path. The imported component builds in every image that holds this component, so the image file does not list it. An image entry at an imported path fails `tect check`. A path may read an option of this component as `$<name>`, so the image chooses the component in this component's block. Several paths go on one line, as `imports \"a\" \"b\"`, or in a block with one path on each line. In the block, a path's own block sets the imported component's options. A value may read this component's options the same way, and an option that the block does not set takes its default. If two components import one component with different values for one option, then `tect check` fails.").example("\"container-runtime\"")
    .arg(Arg::Names, Say::new("`{}` needs a component path", "nothing named",
        "`imports \"identity\"`, one path for each component that this component brings"))
    .props(&[], Say::new("`{}` is not an `imports` property", "not part of the schema",
        "`imports` takes component paths, and a path's block in the block form sets the options"))
    .children(&[IMPORTED], Say::NONE);

#[rustfmt::skip]
const IMPORTED: Node = Node::new("",
    "One component path in an `imports` block, which the node name names. Its block sets the options of that component.").example("\"container-runtime\"")
    .arg(Arg::None, Say::new("`{}` is a second path on one line of the block", "one path per line",
        "give each path its own line"))
    .props(&[], Say::new("`{}` is not an `imports` property", "not part of the schema",
        "a path's block sets the options, as `\"container-runtime\" { storage-driver \"vfs\" }`"))
    .children(&[IMPORTED_OPTION], Say::NONE);

#[rustfmt::skip]
const IMPORTED_OPTION: Node = Node::new("",
    "One option value that the imported component takes, which the node name names.").example("storage-driver \"$storage-driver\"");

#[rustfmt::skip]
const AFTER: Node = Node::new("after",
    "A capability that this component builds after, which the component does not require.").about("Names a capability that this component builds after. If an enabled component provides it, then `tect` builds this component after that component. If no enabled component provides it, then the `after` orders nothing.").example("\"vfio\"")
    .arg(Arg::Names, Say::new("`{}` needs a capability name", "nothing named", ""))
    .props(&[], Say::new("`{}` is not an `after` property", "not part of the schema",
        "`after` takes capability names alone"))
    .children(&[CAPABILITY], Say::NONE);

#[rustfmt::skip]
const PACKAGES: Node = Node::new("packages", "The packages that this component installs.").about("Lists the distribution packages that this component installs with the package manager of the base family. A name can carry a version after one `=`, such as `firefox=$version`. The build spells it `firefox-<version>` for dnf and `firefox=<version>` for apt. Several names go on one line, as `packages \"htop\" \"tmux\"`, or in a block with one name on each line. Each `packages` node installs as one batch, so `enablerepo` sits on the node and covers every name in it.").example("\"htop\" \"tmux\"")
    .arg(Arg::Names, Say::new("`packages` needs at least one name", "nothing to install",
        "`packages \"htop\" \"tmux\"`, or `on \"fedora\" { packages \"vim-enhanced\" }` where \
         the names differ by family"))
    .props(&[
        Prop { name: "enablerepo", kind: Kind::Str,
            desc: "A repository that is enabled for this install only. It works with dnf only, \
                so the batch must resolve to `fedora` or `rhel` alone.",
            say: Say::NONE,
            missing: Say::NONE },
    ], Say::new("unknown `packages` property `{}`", "not part of the schema",
        "`packages` accepts `enablerepo`"))
    .children(&[PACKAGE], Say::NONE);

/// Matches one name in the block of `packages` or `package-groups`. The node
/// installs its names as one batch, so a name carries no property of its own.
#[rustfmt::skip]
const PACKAGE: Node = Node::new("",
    "One name in the block, which the node name names.").example("\"htop\"")
    .arg(Arg::None, Say::new("`{}` is a second name on one line of the block", "one name per line",
        "give each name its own line"))
    .props(&[], Say::new("`{}` describes the whole batch, and this sits on one name", "on one name",
        "move it onto the block node, or write the name in its own `packages` node with it"))
    .children(&[], Say::new("a package name holds no `{}`", "not part of the schema", ""));

#[rustfmt::skip]
const PACKAGE_GROUPS: Node = Node::new("package-groups",
    "The package groups that this component installs. Package groups work with dnf only, so an \
     ungated `package-groups` needs a component that targets only `fedora` or `rhel`.").example("\"kde-desktop\"")
    .arg(Arg::Names, Say::new("`package-groups` needs at least one name", "nothing to install",
        "`package-groups \"kde-desktop\"`"))
    .props(&[
        Prop { name: "enablerepo", kind: Kind::Str,
            desc: "A repository that is enabled for this install only.",
            say: Say::NONE,
            missing: Say::NONE },
    ], Say::new("unknown `package-groups` property `{}`", "not part of the schema",
        "`package-groups` accepts `enablerepo`"))
    .children(&[PACKAGE], Say::NONE);

#[rustfmt::skip]
const COPR: Node = Node::new("copr",
    "The `owner/project` name of a COPR repository that this component enables for its own installs. \
     `copr` works on Fedora only.").example("\"owner/project\"")
    .arg(Arg::Str, Say::new("`copr` needs one owner/project string", "nothing named",
        "`copr \"owner/project\"`"))
    .unique(Say::new("copr `{}` is declared twice", "already declared above", ""))
    .props(&[], Say::new("`{}` is not a `copr` property", "not part of the schema",
        "`copr \"owner/project\"`"));

#[rustfmt::skip]
const SATISFIES: Node = Node::new("satisfies",
    "An audit declaration of the benchmarks and rules that this component claims to harden. `tect` \
     records the claim and certifies nothing, and the scan checks it after the build.").about("Claims that this component meets rules of a hardening benchmark, such as STIG. The compliance scan checks each claim against the built image, so a false claim shows.")
    .once("a component makes one claim set per gate; two blocks in one place split it")
    .children(BENCHMARKS, Say::NONE)
    .lists(&[
        ("`tect generate` writes every claim into `generated/plan.json`. The compliance job in \
          `.github/workflows/build.yml` then:", &[
            "resolves each number to an XCCDF rule ID through the SSG datastream;",
            "scans the pushed image and compares each claim with the scan result;",
            "reports a number that maps to no rule as a failed declaration;",
            "reports a rule that the image fails as a false claim;",
            "reports a failed rule as a composition failure, if the overlay of another component \
             owns the final copy of a file that this component ships. `plan.json` carries \
             `overlay_overridden` for that case;",
            "skips a target whose components declare nothing, and says so;",
            "reads `.components[]` only and never `.suppressed[]`, because a suppressed component adds \
             no layer.",
        ]),
    ]);

/// The rows inside a `satisfies` block, which read the same wherever the block
/// is declared: a component gates one, an image's base declares one flat.
#[rustfmt::skip]
pub(crate) const BENCHMARKS: &[Node] = &[
    Node::new("", "One benchmark, which the node name names, with the rule numbers that it covers. The set of benchmarks is open.").example("stig \"RHEL-09-232010\"")
        .arg(Arg::Strs, Say::new("`{}` has no rules listed", "nothing to cover", ""))
        .unique(Say::new("benchmark `{}` is declared twice", "already declared above", "")),
];

/// The gate. Nodes inside are taken only on the families it names; nodes
/// outside any gate are taken on every family the component targets. Files are
/// gated by a directory of the same name -- see `layout::family_dir` -- and
/// never by a node here.
#[rustfmt::skip]
const ON: Node = Node::new("on",
    "The declarations that the build takes only on the named base families.").about("Holds the declarations that differ by base family, such as package names that differ between Fedora and Debian. One `manifest.kdl` can then target several families.").example("\"debian\" \"ubuntu\"")
    .arg(Arg::Strs, Say::new("`on` needs at least one family name", "nothing named",
        "`on \"debian\" \"ubuntu\" { packages \"vim\" }`; one gate takes as many families as \
         share the declaration"))
    .props(&[], Say::new("`{}` is not an `on` property", "not part of the schema",
        "`on` takes family names alone"))
    .children(GATED, Say::new("`{}` is not gated by family", "not allowed in an `on` block",
        "an `on` block holds `packages`, `package-groups`, `copr`, `requires`, `imports`, `after` \
         and `satisfies`; `component.sh`, `finalize.sh` and `files/` are gated by putting them in a \
         `<family>/` directory, and everything else a manifest declares is family-neutral"))
    .notes(&[
        "A declaration outside a gate applies to every family the component targets. A \
         declaration inside a gate applies to the families that the gate names.",
        "One gate takes several family names, so the user writes a list that two families share \
         once.",
        "`provides` stays outside a gate. A capability that one family offers and another does \
         not belongs in a second component.",
        "`tect` reads every gate whether or not the image builds for it, so the manifest gets \
         the same checks on each family it claims.",
        "The build drops each gated declaration for a family that it does not build. A \
         Fedora-only `after` does not order the component after a Debian provider, and a Debian \
         scan does not map a Fedora rule number.",
        "If a gate names a family that the component does not target, then `tect` refuses the \
         gate.",
    ]);

/// Names the family that a base installs as. A base may name a family that
/// the gates do not know, because an unmeasured family installs off its own
/// `layout`.
#[rustfmt::skip]
const BASE_FAMILY: Node = Node::new("family",
    "The family that an image on this base installs as.").about("Names the base family, such as `fedora` or `debian`, which picks the package manager and the family's defaults. If a base names no family, then `tect check` holds no component to its `targets` on that base.").example("\"fedora\"")
    .arg(Arg::Str, Say::new("`family` needs a family name", "no family given",
        "`family \"fedora\"`, the family that an image on this base installs as"))
    .once("")
    .children(&[], Say::new("`family` holds no `{}`", "not allowed in `family`",
        "`family` names a base's family; a gate is `on \"fedora\" { packages \"vim\" }`"));

#[rustfmt::skip]
const BASE_BOOTLOADER: Node = Node::new("bootloader",
    "The bootloaders that an image on this base can install, with the default first.").about("Lists the bootloaders that the base carries. `tect create image` offers them when it writes the image's `layout`. If a base names none, then `tect create image` chooses no bootloader, and the user names one in the `layout`.").example("\"grub2\"")
    .arg(Arg::Strs, Say::new("`bootloader` needs a name", "no bootloader given",
        "`bootloader \"grub2\"`, or `bootloader \"grub2\" \"systemd\"` where the base \
         carries both"))
    .once("");

#[rustfmt::skip]
const BASE_SCAP_CONTENT: Node = Node::new("scap-content",
    "The SSG datastream file that a scan of an image on this base reads.").about("Names the SCAP content file that measures an image on this base, as a filename in the content directory. If a base names no content, then the scan reads the default content of the base's family. Each EL release publishes its own datastream, so the `rhel` family has no default. `tect check` refuses an image that conforms to a benchmark while its base resolves to no content.").example("\"ssg-fedora-ds.xml\"")
    .arg(Arg::Str, Say::new("`scap-content` needs a filename", "no file given",
        "`scap-content \"ssg-cs10-ds.xml\"`, a datastream in the SSG content directory"))
    .once("");

/// The manifest's grammar, and the whole of it.
#[rustfmt::skip]
pub const COMPONENT: Node = Node::new("component",
    "One component, which declares what it builds on, what it needs and what it installs.").about("A component is one reusable part of an image, such as a desktop, a kernel or a set of tools. `manifest.kdl` declares its schema version, what the component needs and provides, what it installs and which families it targets, and the files beside it do the work.").scaffolds(&["schema-version", "description", "targets", "packages"])
    .children(&[
        FILE_SCHEMA_VERSION,
        Node::new("description", "One line that names the component in the resolved build summary.").example("\"Traditional CLI utilities\"")
            .arg(Arg::Str, Say::new("`description` needs a string", "no description given", ""))
            .once(""),
        Node::new("targets",
            "The base families that this component builds on, which the image's `family` must match. Omit it to target every family.").about("Lists the base families that the component works on. `tect check` refuses the component on an image whose base family is not in the list, so a portability gap shows before the build. A component with no `targets` declaration is base-agnostic and targets every family.").example("\"fedora\" \"debian\"")
            .arg(Arg::Strs, Say::new("`{}` needs at least one family", "nothing named",
                "`targets \"fedora\"`, one name for each base family the component builds on"))
            .props(&[], Say::new("`{}` is not a `targets` property", "not part of the schema",
                "`targets` takes family names alone")),

        Node::new("provides", "A capability that this component provides for the components that require it.").about("Names a capability that this component adds to the image, such as `flatpak`. Another component `requires` it by name. If a name declares `witness`, then the build checks the finished image for that file. A name may have `/`-separated segments, and `rpm/dnf5` also meets a requirement for `rpm`. Several names go on one line, as `provides \"a\" \"b\"`, or in a block with one name on each line, where each name carries its own properties.").example("\"flatpak\"")
            .arg(Arg::Names, Say::new("`{}` needs a capability name", "nothing named", ""))
            .props(PROVIDES_PROPS, Say::new("unknown `provides` property `{}`", "not part of the schema",
                "`provides` accepts `witness`, `build-only` and `exclusive`"))
            .children(&[PROVIDED], Say::NONE)
            .notes(&[
                "The build checks the finished image for the witness file.",
                "A `provides` without `witness`, such as `rechunking`, has no witness. `tect` \
                 holds no default path for a name.",
                "`tect validate-image` checks the finished image for the witness of each name \
                 that the base or a component provides with a `witness`. `base-sig-probe` reads \
                 those witnesses when it measures a base, except the ones of a component that \
                 meets a base requirement or that the base imports.",
                "The witness of `luks-initramfs` is inside an archive. If a target declares it, \
                 then the build validates the initramfs with `lsinitrd`. If no target declares \
                 it, then the installer does not offer root encryption.",
                "Declare `luks-initramfs` in a custom base or component only if its initramfs \
                 unlocks LUKS. `tect build` then proves that the binary is present before the \
                 installer offers it.",
                "Many enabled components may provide one capability. If one of them declares it \
                 `exclusive=#true`, then `tect check` refuses a second provider that builds \
                 into the same image or flavour. `tect import component` asks whether to replace \
                 the first. Providers in sibling flavours never build together.",
                "A capability name is lower-case letters, digits and dashes, and starts with a \
                 letter. The generated graph writes the name unquoted into a mermaid label and a \
                 markdown table cell.",
            ]),
        REQUIRES,
        IMPORTS,
        AFTER,
        Node::new("removes", "A capability that this component withdraws from the image.").about("Names a capability that the base or another component provides and that this component takes away, such as a tool that its script uninstalls. The image then no longer claims the capability. `tect` builds this component after every provider of the name. `tect check` refuses a `removes` that nothing in the image provides, and it refuses every component that requires a removed name. `tect validate-image` checks that the witness of a removed name is absent from the finished image.").example("\"bootupctl\"")
            .arg(Arg::Names, Say::new("`{}` needs a capability name", "nothing named",
                "`removes \"bootupctl\"`, one name for each capability this component takes away"))
            .props(&[], Say::new("`{}` is not a `removes` property", "not part of the schema",
                "`removes` takes capability names alone"))
            .children(&[CAPABILITY], Say::NONE),

        Node::new("overrides", "An absolute path that this component replaces on purpose.").about("Declares that this component replaces a file that an earlier component also ships. Without it, `tect check` reports the two overlays as a collision. `tect check` also reports an override that no earlier component collides with, so an override cannot outlive its reason.").example("\"/etc/containers/policy.json\"")
            .arg(Arg::Strs, Say::NONE)
            .props(&[], Say::new("`{}` is not an `overrides` property", "not part of the schema",
                "")),
        Node::new("mode", "The octal file mode that one path in the overlay of this component takes.").about("Sets the file mode of one file that the `files/` overlay of the component installs, such as `0600` for a file that only root reads.").example("\"/etc/audit/rules.d/audit.rules\" \"0600\"")
            .arg(Arg::StrPair("path", "octal mode"),
                Say::new("`mode` needs one path and one octal file mode", "incomplete",
                    "`mode \"/etc/example.conf\" \"0644\"`"))
            .unique(Say::new("mode for `{}` is declared twice", "already declared above", ""))
            .props(&[], Say::new("`{}` is not a `mode` property", "not part of the schema",
                "`mode \"/etc/example.conf\" \"0440\"`")),

        KEY,

        Node::new("secret", "A build secret that the layer of this component mounts.").about("Names a build secret that the layer of this component reads, such as a signing key. The layer mounts it at `/run/secrets/<name>`, so the secret is there while the component builds and never lands in the image.").example("\"mok_privkey\"")
            .arg(Arg::Strs, Say::new("`{}` needs a name", "nothing named", "")),
        Node::new("arg", "A build argument that the layer of this component reads.").about("Names a build argument that the layer of this component reads as an environment variable, such as the kernel version to install.").example("\"KERNEL\"")
            .arg(Arg::Strs, Say::new("`{}` needs a name", "nothing named", "")),
        Node::new("helpers", "Files from this component that every component layer mounts by basename into `/ctx/lib`.").about("Shares shell helpers from this component with every other component. The build mounts each listed file by its basename into `/ctx/lib` in every component layer, so another component can source it.").example("\"lib/family.sh\"")
            .arg(Arg::Strs, Say::new("`helpers` needs a path", "nothing named", "")),

        Node::new("allow-verify",
            "One `tect validate-image` diagnostic that the check accepts on one unit, while the check still covers every other diagnostic and unit of the image.").about("`tect validate-image` checks the systemd units of the finished image. If one unit of this component raises an expected diagnostic, then this accepts that one diagnostic on that one unit. The check still covers every other diagnostic and unit.").example("\"man-page-missing\" unit=\"plasmalogin.service\"")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "unit", kind: Kind::Str,
                    desc: "The unit the exception applies to.",
                    say: Say::new("`unit` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `allow-verify` property `{}`", "not part of the schema",
                "`allow-verify` accepts `unit`")),

        Node::new("refuses",
            "One benchmark rule that this component leaves unsatisfied on purpose.").about("A hardening rule can break what a component does. This records that the component leaves one rule unsatisfied on purpose, and `because=` says why. Remediation then does not set the rule, and the choice is visible to every image.").example("\"grub2_nousb_argument\" because=\"it removes the keyboard\"")
            .arg(Arg::Str, Say::new("`refuses` needs a rule ID", "nothing named",
                "`refuses \"grub2_nousb_argument\" because=\"it removes the keyboard\"`"))
            .unique(Say::new("`{}` is refused twice", "already refused above", ""))
            .props(&[
                Prop { name: "because", kind: Kind::Str,
                    desc: "Why the component leaves the rule unsatisfied.",
                    say: Say::new("`because` must be a string", "not a string", ""),
                    missing: Say::NONE },
            ], Say::new("unknown `refuses` property `{}`", "not part of the schema",
                "`refuses` accepts `because`"))
            .notes(&[
                "Remediation does not set a refused rule. If an image declares \
                 `allow-remediation` for the rule, then remediation can set it.",
            ]),

        Node::new("collects", "A filename that this component gathers from every component that ships one. A collector claims one filename across the image.").about("Builds one file from parts that many components ship, such as a list of Flatpak apps. This component claims the file name, every component that ships a file of that name contributes a part, and the finalize phase assembles the parts at `into=`.").example("\"flatpaks.list\" into=\"/usr/share/flatpak-defaults/apps.list\" priority=500")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "into", kind: Kind::Str,
                    desc: "The absolute path that receives the assembled file.",
                    say: Say::NONE,
                    missing: Say::NONE },
                Prop { name: "priority", kind: Kind::Int(PRIORITIES.0, PRIORITIES.1),
                    desc: "The position of a contribution that declares no priority.",
                    say: PRIORITY,
                    missing: Say::NONE },
            ], Say::new("unknown `collects` property `{}`", "not part of the schema",
                "`collects` accepts `into` and `priority`"))
            .notes(&[
                "The build stages each contribution as `<into>.d/NNNN-<component>.part`. The \
                 finalize phase assembles the parts in that order, so the build order of the \
                 contributors does not change the assembled file.",
                "A component ships at most one copy of a collected file. The most specific copy \
                 wins. The order is `debian/`, then `deb/`, then the component root.",
            ]),
        Node::new("contributes", "A file that this component ships for another component to collect.").example("\"flatpaks.list\"")
            .arg(Arg::Str, Say::NONE)
            .props(&[
                Prop { name: "priority", kind: Kind::Int(PRIORITIES.0, PRIORITIES.1),
                    desc: "The position that this file takes in the assembled file.",
                    say: PRIORITY,
                    missing: Say::NONE },
            ], Say::new("unknown `contributes` property `{}`", "not part of the schema",
                "`contributes` accepts `priority`")),

        Node::new("fragment",
            "Where the component's `Containerfile.inc` goes in relation to the generated layer.").about("A component whose needs the fields cannot express ships a `Containerfile.inc`, which the build adds verbatim. This places those lines relative to the generated layer.").example("position=\"after\"")
            .arg(Arg::None, Say::new("`fragment` takes no arguments", "unexpected value",
                "`fragment position=\"after\"`"))
            .once("")
            .props(&[
                Prop { name: "position", kind: Kind::One(&["before", "after", "tail"]),
                    desc: "Where the fragment goes. `before`, the default, puts it above the \
                        generated block. `after` puts it below the block. `tail` puts it below the \
                        finalize layer, where the lineage stage has ended and can be named.",
                    say: Say::new("`position` must be \"before\", \"after\" or \"tail\"", "not a position",
                        "before, the default, puts the fragment above the generated block; after \
                         puts it below; tail puts it below the finalize layer, where the lineage \
                         stage has ended and can be named"),
                    missing: Say::NONE },
                Prop { name: "standard-layer", kind: Kind::Bool,
                    desc: "Whether the build emits the generated block.",
                    say: Say::new("`standard-layer` must be #true or #false", "not a boolean", ""),
                    missing: Say::NONE },
            ], Say::new("unknown fragment property `{}`", "not part of the schema",
                "a fragment accepts `position` and `standard-layer`"))
            .notes(&[
                "The build places the fragment verbatim and replaces `@COMPONENT@` with \
                 `/components/<dir>`, the component directory in the build context. The mounts of the \
                 component layer use the same path, so a `COPY --from=ctx` needs it to reach a file \
                 that the component ships.",
                "Every `RUN` bind-mounts over `/etc/hostname`, `/etc/hosts` and \
                 `/etc/resolv.conf`. A fragment writes those three files with a `COPY --from=ctx`.",
            ]),

        options::OPTION,
        options::VARIANT,
        asset::ASSET,
        Node::new("network", "The declaration that the script step of this component reaches the network.").about("Declares that the scripts of this component need the network. A repository with a `strict` script rule opens the network only for the components that declare it.").example("\"scripts\"")
            .arg(Arg::One(&["scripts"]), Say::new("`{}` is not a step this component declares",
                "not a step",
                "`network \"scripts\"`; a `packages`, `copr` or repo file already declares the \
                 package step"))
            .once("")
            .notes(&[
                "Only a `strict` script rule in `repo.kdl` reads this node.",
                "The package step needs no declaration, because `packages`, `copr` or a repo \
                 file already declares it.",
            ]),

        PACKAGES,
        PACKAGE_GROUPS,
        COPR,
        SATISFIES,

        ON,

        Node::new("base", "The image reference that an image on this base builds from. It makes the component a base.").about("Names the upstream bootc image that a base stands for, and makes the component a base. A base provides `base-image` exclusively, so an image holds one. The reference usually reads the base's `version` option, as `base \"quay.io/fedora/fedora-bootc:$version\"` does, so the image sets the version on the base's entry. A block after the reference holds what the base states about itself. A base with no block is valid, and each field it leaves out takes the effect its own row states.").example("\"quay.io/fedora/fedora-bootc:$version\"")
            .arg(Arg::Str, Say::new("`base` needs an image reference", "no image given",
                "`base \"quay.io/fedora/fedora-bootc:$version\"`, the reference a build on this base starts from"))
            .once("")
            .props(&[], Say::new("`{}` is not a `base` property", "not part of the schema",
                "`base` takes its reference, and its block holds `family`, `bootloader` and `scap-content`"))
            .children(&[BASE_FAMILY, BASE_BOOTLOADER, BASE_SCAP_CONTENT], Say::new("`base` holds no `{}`", "not allowed in `base`",
                "a base's block holds `family`, `bootloader` and `scap-content`; an image sets \
                 `digest` and `signed` in the base's segment")),
    ], Say::new("unknown node `{}`", "not part of the schema",
        "docs/schema/component.md documents every node a manifest may hold"))
    .notes(&[
        "`requires` and `after` decide the build order. The order of the image list only breaks \
         ties.",
        "If nothing provides a `requires`, then `tect check` fails and names every component that \
         would satisfy it.",
    ]);

/// `owner/project` split into its two segments, or None when it is not that.
/// The one place a COPR name is read, so `copr` and an `enablerepo` naming one
/// cannot disagree about the shape.
fn copr_parts(value: &str) -> Option<(&str, &str)> {
    let (owner, project) = value.split_once('/')?;
    let both_safe = bad_token(owner).is_none() && bad_token(project).is_none();
    both_safe.then_some((owner, project))
}

/// Reads one `packages` or `package-groups` node in either form. Each name is
/// held to what a RUN line can hold. The node yields one batch per family that
/// the enclosing gate names. A node outside a gate yields one batch naming no
/// family, which `spread` resolves once `targets` has been read.
fn batch(
    node: &KdlNode,
    gate: &[String],
    src: &Source,
    issues: &mut Issues,
    noun: &str,
) -> Vec<PackageGroup> {
    let mut names: Vec<String> = Vec::new();
    for named in crate::parse::names(node) {
        let value = named.name;
        // The manifest parse checks a reference against the options.
        // Resolution checks its value once the image sets it.
        let literal = resolve_options::interpolate(value, |_| "x".to_string());
        if let Some(problem) = bad_package(&literal) {
            issues.push(
                Issue::new(format!("{noun} name `{value}` {problem}"), src)
                    .at(named.span, "would not survive the RUN line")
                    .help(TOKEN_HELP),
            );
            continue;
        }
        names.push(value.to_string());
    }
    if names.is_empty() {
        // An empty node is what the schema table already reported.
        return Vec::new();
    }

    let mut enablerepo: Option<String> = None;
    if let Some(span) = prop_span(node, "enablerepo") {
        match prop(node, "enablerepo").filter(|v| !v.is_empty()) {
            // `owner/project` names one of this component's `copr` nodes and is
            // turned into the selector below, once every node is read.
            Some(repo) => match bad_token(repo).filter(|_| copr_parts(repo).is_none()) {
                Some(problem) => issues.push(
                    Issue::new(format!("repo ID `{repo}` {problem}"), src)
                        .at(span, "would not survive the RUN line")
                        .help(TOKEN_HELP),
                ),
                None => enablerepo = Some(repo.to_string()),
            },
            None => issues.push(
                Issue::new("`enablerepo` needs a repo ID string", src).at(span, "not a string"),
            ),
        }
    }

    // No family here is not "no families": it is every family the component
    // targets, which `spread` fills in.
    let families: Vec<String> = match gate.is_empty() {
        true => vec![String::new()],
        false => gate.to_vec(),
    };
    families
        .into_iter()
        .map(|family| PackageGroup {
            family,
            packages: names.clone(),
            enablerepo: enablerepo.clone(),
            span: node.name().span().into(),
        })
        .collect()
}

/// A batch declared outside a gate installs on every family the component
/// targets. It waits for the whole manifest because `targets` may be written
/// below the `packages` leaning on it, and because a dnf-only diagnostic is
/// about the families a batch resolved to.
fn spread(
    batches: &mut Vec<PackageGroup>,
    targets: &[String],
    block: &str,
    dnf_only: bool,
    src: &Source,
    issues: &mut Issues,
) {
    // A gate naming two families is two batches off one declaration, so what is
    // said about it is said once, against the node.
    let mut said: Option<Span> = None;
    for mut batch in std::mem::take(batches) {
        let families: Vec<String> = match (batch.family.is_empty(), targets.is_empty()) {
            (false, _) => vec![std::mem::take(&mut batch.family)],
            (true, false) => targets.to_vec(),
            (true, true) => FAMILIES
                .iter()
                .map(|family| (*family).to_string())
                .collect(),
        };
        let first = said.replace(batch.span) != Some(batch.span);
        if let Some(stray) = families.iter().find(|family| !rpm(family)) {
            if dnf_only {
                if first {
                    issues.push(
                        Issue::new(format!("`{block}` is dnf's, and this one covers `{stray}`"), src)
                        .at(batch.span, "no group installer on this family")
                            .help("only the dnf adapter installs package groups; a component that also targets a deb family declares it inside `on \"fedora\" \"rhel\"`"),
                    );
                }
                continue;
            }
            if batch.enablerepo.is_some() {
                if first {
                    issues.push(
                        Issue::new(format!("`enablerepo` is dnf's, and this batch covers `{stray}`"), src)
                            .at(batch.span, "no repo to enable on this family")
                            .help("declare the batch inside `on \"fedora\" \"rhel\"`; a Debian or Ubuntu install takes the base image's configured sources"),
                    );
                }
                batch.enablerepo = None;
            }
        }
        batches.extend(families.into_iter().map(|family| PackageGroup {
            family,
            packages: batch.packages.clone(),
            enablerepo: batch.enablerepo.clone(),
            span: batch.span,
        }));
    }
}

/// Reads the facts a base declares in its `base` block. A component that
/// declares `base` is a base. A base with no `family` takes only the ungated
/// content of each component.
fn base_facts(nodes: &[KdlNode], src: &Source, issues: &mut Issues) -> Option<BaseFacts> {
    let reference = nodes.iter().find(|node| node.name().value() == "base")?;
    let facts = base_block(reference);
    if facts.image.is_empty() {
        return None;
    }
    for name in &facts.bootloaders {
        if !crate::parse::image::BOOTLOADERS.contains(&name.as_str()) {
            issues.push(
                Issue::new(
                    format!("`{name}` is not a bootloader the installer installs"),
                    src,
                )
                .at(
                    child(reference, "bootloader")
                        .map_or(Span::default(), |node| node.name().span().into()),
                    "not a bootloader",
                )
                .help("the installer writes `grub2` or `systemd`"),
            );
        }
    }
    Some(facts)
}

/// Reads a `base` node's reference and the facts in its block, as the
/// manifest writes them.
fn base_block(reference: &KdlNode) -> BaseFacts {
    let image = string_arg(reference).unwrap_or_default().to_string();
    BaseFacts {
        template: image.clone(),
        image,
        family: text(reference, "family"),
        bootloaders: child(reference, "bootloader")
            .map(|node| string_args(node).into_iter().map(str::to_string).collect())
            .unwrap_or_default(),
        scap_content: text(reference, "scap-content"),
    }
}

/// Lists the names that every `want` node among `nodes` holds.
fn listed<'a>(nodes: impl IntoIterator<Item = &'a KdlNode>, want: &str) -> Vec<String> {
    nodes
        .into_iter()
        .filter(|node| node.name().value() == want)
        .flat_map(names)
        .map(|named| named.name.to_string())
        .collect()
}

/// Holds every `$<name>` reference in a manifest to the options it declares.
/// Only a string option interpolates, because a bool or a list value has no
/// one spelling inside a package name or a component path.
fn check_references(top: &[KdlNode], options: &[Opt], src: &Source, issues: &mut Issues) {
    let mut nodes: Vec<&KdlNode> = Vec::new();
    for node in top {
        match node.name().value() {
            "on" => nodes.extend(kids(node)),
            _ => nodes.push(node),
        }
    }
    let mut texts: Vec<(&str, Span)> = Vec::new();
    for node in nodes {
        match node.name().value() {
            "base" => texts.extend(string_arg(node).map(|text| (text, node.name().span().into()))),
            "packages" | "package-groups" => texts.extend(
                names(node)
                    .into_iter()
                    .map(|named| (named.name, named.span)),
            ),
            "imports" => {
                for named in names(node) {
                    texts.push((named.name, named.span));
                    for kid in named.kids {
                        let span: Span = kid.name().span().into();
                        texts.extend(string_args(kid).into_iter().map(|text| (text, span)));
                    }
                }
            }
            _ => {}
        }
    }
    for (text, span) in texts {
        let mut names: Vec<String> = Vec::new();
        resolve_options::interpolate(text, |name| {
            names.push(name.to_string());
            String::new()
        });
        for name in names {
            let problem = match options
                .iter()
                .find(|opt| opt.name == name)
                .map(|opt| opt.ty)
            {
                Some(OptType::String) => continue,
                Some(OptType::Bool) => format!("`${name}` reads a bool option"),
                Some(OptType::List) => format!("`${name}` reads a list option"),
                None => format!("`${name}` names no option of this component"),
            };
            issues.push(
                Issue::new(problem, src)
                    .at(span, "not a string option")
                    .help(format!(
                        "only a string option interpolates: declare `option \"{name}\" type=\"string\"`"
                    )),
            );
        }
    }
}

/// The families one `family` gate names, each held to the set this repository
/// builds on. A gate takes as many as share the declaration.
fn gate_families(node: &KdlNode, src: &Source, issues: &mut Issues) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in string_args(node) {
        if !FAMILIES.contains(&name) {
            issues.push(
                Issue::new(format!("unknown base family `{name}`"), src)
                    .at(node.name().span(), "not a family this repository builds on")
                    .help(format!("known families: {}", FAMILIES.join(", "))),
            );
            continue;
        }
        if out.iter().any(|seen| seen == name) {
            issues.push(
                Issue::new(format!("`{name}` is named twice on one gate"), src)
                    .at(node.name().span(), "already named here"),
            );
            continue;
        }
        out.push(name.to_string());
    }
    out
}

/// The schema version that `repo.kdl` declares, which every manifest must match.
struct RepoSchema<'a> {
    version: Option<u32>,
    src: &'a Source,
}

/// What the gated nodes have put into a component so far, so a gate this image is
/// not built for can be read for its diagnostics and then dropped. `packages`
/// and `package-groups` are absent because a batch carries the family it
/// resolved to and is filtered where it is used.
struct Gated {
    coprs: usize,
    requires: usize,
    imports: usize,
    after: usize,
    satisfies: usize,
}

/// The calling function passes the schema table's range for `key`. If `key` is
/// declared and this returns `None`, then the table already reported it.
fn ranged(node: &KdlNode, key: &str, (low, high): (i128, i128)) -> Option<u32> {
    int_prop(node, key)
        .filter(|value| (low..=high).contains(value))
        .and_then(|value| u32::try_from(value).ok())
}

fn priority(node: &KdlNode) -> Option<u32> {
    ranged(node, "priority", PRIORITIES)
}

/// Whether a `priority=` is there but not a number the schema accepts, which is
/// its diagnostic. A second one here would say it twice.
fn bad_priority(node: &KdlNode) -> bool {
    prop_span(node, "priority").is_some() && priority(node).is_none()
}

fn regular_overlay_file(root: &Path, path: &str) -> bool {
    let mut at = root.to_path_buf();
    let mut parts = path.trim_start_matches('/').split('/').peekable();
    while let Some(part) = parts.next() {
        at.push(part);
        let Ok(metadata) = at.symlink_metadata() else {
            return false;
        };
        if match parts.peek() {
            Some(_) => !metadata.is_dir(),
            None => !metadata.is_file(),
        } {
            return false;
        }
    }
    true
}

impl Component {
    pub fn load(
        entry: &Entry,
        image: &Image,
        root: &Path,
        disk: &Disk,
        repo_version: Option<u32>,
        repo_src: &Source,
        issues: &mut Issues,
    ) -> Option<Self> {
        let dir_rel = &entry.path;
        let file = root
            .join(layout::COMPONENTS)
            .join(dir_rel)
            .join(layout::MANIFEST_FILE)
            .display()
            .to_string();

        let family = image.base.as_ref().map(|base| base.family.as_str());
        let mut component = match &entry.inline {
            Some(_) if layout::is_component(&layout::component(root, dir_rel)) => {
                issues.push(
                    Issue::new(
                        format!(
                            "`{}` is defined inline, and components/{dir_rel} is a component too",
                            entry.path
                        ),
                        &image.src,
                    )
                    .at(entry.span, "defined here")
                    .help(format!(
                        "list components/{dir_rel} by its segments alone, or give the inline \
                         definition a path of its own"
                    )),
                );
                return None;
            }
            Some(spans) => {
                let nodes = crate::parse::image::nodes_at(&image.src, spans);
                Self::parse_inline(&entry.path, root, &nodes, family, &image.src, issues)
            }
            None => match std::fs::read_to_string(&file) {
                Ok(text) => Self::parse_against(
                    &entry.path,
                    dir_rel,
                    root,
                    text,
                    family,
                    RepoSchema {
                        version: repo_version,
                        src: repo_src,
                    },
                    issues,
                )?,
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                    issues.push(
                        Issue::new(format!("{file} cannot be read: {err}"), &image.src)
                            .at(entry.span, "manifest.kdl unreadable"),
                    );
                    return None;
                }
                Err(_) if disk.is_component(dir_rel) => {
                    Self::bare(&entry.path, dir_rel, root, issues)
                }
                Err(_) => {
                    issues.push(
                        Issue::new(format!("`{}` is not a component", entry.path), &image.src)
                            .at(entry.span, "nothing to build")
                            .help(format!(
                                "give components/{dir_rel} a manifest.kdl, a component.sh, a \
                                 repo.sh, a packages.sh, a finalize.sh, a files/ directory, a \
                                 Containerfile.inc or a file that another component collects, \
                                 or define it inline with a manifest node such as `packages`"
                            )),
                    );
                    return None;
                }
            },
        };
        component.flavour = entry.flavour.clone();
        component.resolved = resolve_options::resolve(
            &component.options,
            &component.variants,
            &component.src,
            entry,
            image,
            issues,
        );
        resolve_options::apply(&mut component, entry, image, issues);
        // A component nested inside another component builds on the one that holds
        // it. The nearest enclosing component becomes a requirement on its path.
        // An organising directory between them is no component and adds nothing.
        let mut parts: Vec<&str> = entry.path.split('/').collect();
        parts.pop();
        while !parts.is_empty() {
            let parent = parts.join("/");
            if disk.is_component(&parent) || image.defined.contains(&parent) {
                component.requires.push(Decl {
                    name: parent,
                    span: Span::default(),
                });
                break;
            }
            parts.pop();
        }
        Some(component)
    }

    /// Builds a component that has no manifest.kdl. It declares no family, so the
    /// family gate does not apply to it.
    fn bare(path: &str, dir_rel: &str, root: &Path, issues: &mut Issues) -> Self {
        let dir = layout::component(root, dir_rel);
        let src = Source::new(dir.display().to_string(), String::new());
        let component = Self::empty(path, dir_rel, root, &src);
        Self::rest(component, path, root, &dir, &[], &src, issues)
    }

    /// A component that declares nothing, with what its directory holds.
    fn empty(path: &str, dir_rel: &str, root: &Path, src: &Source) -> Self {
        let dir = layout::component(root, dir_rel);
        Self {
            path: path.to_string(),
            dir: dir_rel.to_string(),
            description: String::new(),
            targets: Vec::new(),
            provides: Vec::new(),
            exclusive: Vec::new(),
            requires: Vec::new(),
            base: None,
            imports: Vec::new(),
            removes: Vec::new(),
            after: Vec::new(),
            policies: [layout::SELINUX, layout::APPARMOR]
                .iter()
                .filter(|policy| !policy.files(&dir).is_empty())
                .map(|policy| policy.capability)
                .collect(),
            files: Vec::new(),
            overrides: Vec::new(),
            verify_exceptions: Vec::new(),
            refuses: Vec::new(),
            flavour: None,
            collects: Vec::new(),
            contributes: Vec::new(),
            modes: Vec::new(),
            keys: Vec::new(),
            refused_keys: Vec::new(),
            secrets: Vec::new(),
            args: Vec::new(),
            options: Vec::new(),
            variants: Vec::new(),
            assets: Vec::new(),
            packages: Vec::new(),
            groups: Vec::new(),
            coprs: Vec::new(),
            helpers: Vec::new(),
            satisfies: Vec::new(),
            resolved: Vec::new(),
            fragment: std::fs::read_to_string(dir.join("Containerfile.inc")).ok(),
            fragment_position: Position::default(),
            standard_layer: true,
            content: crate::provenance::lock::hash(&dir),
            imported: crate::provenance::lock::entry(root, dir_rel),
            // Every family, since a manifest is read without one: a `repo.sh`
            // gated to a family this build is not for is still an archive the
            // component configures somewhere.
            repo: layout::FAMILY_DIRS
                .iter()
                .map(|(gated, _)| dir.join(gated).join(layout::REPO_SCRIPT))
                .chain(std::iter::once(dir.join(layout::REPO_SCRIPT)))
                .any(|at| at.is_file()),
            network: None,
            access: Access::OPEN,
            src: src.clone(),
        }
    }

    /// Everything a manifest says on its own, so a component no image lists is
    /// still held to the schema.
    fn parse_against(
        path: &str,
        dir_rel: &str,
        root: &Path,
        text: String,
        family: Option<&str>,
        repo: RepoSchema,
        issues: &mut Issues,
    ) -> Option<Self> {
        let RepoSchema {
            version: repo_version,
            src: repo_src,
        } = repo;
        let dir = layout::component(root, dir_rel);
        let file = dir.join(layout::MANIFEST_FILE).display().to_string();

        let src = &Source::new(&file, text.clone());
        let doc: KdlDocument = match text.parse() {
            Ok(doc) => doc,
            Err(err) => {
                issues.push(syntax_issue(&err, &file, src));
                return None;
            }
        };

        check_doc(&doc, &COMPONENT, src, issues);
        crate::parse::check_file_version(&doc, repo_version, repo_src, src, issues);
        Some(Self::parse_doc(
            path,
            dir_rel,
            root,
            doc.nodes(),
            family,
            src,
            issues,
        ))
    }

    /// Reads a component that an image file defines inline. The image file
    /// declares the schema version, so the definition declares none.
    pub(crate) fn parse_inline(
        path: &str,
        root: &Path,
        nodes: &[KdlNode],
        family: Option<&str>,
        src: &Source,
        issues: &mut Issues,
    ) -> Self {
        crate::parse::schema::check_part(nodes, &COMPONENT, src, issues);
        if let Some(version) = nodes
            .iter()
            .find(|node| node.name().value() == "schema-version")
        {
            issues.push(
                Issue::new(format!("`{path}` declares a schema version inline"), src)
                    .at(version.name().span(), "set by the image file")
                    .help("the image file's own `schema-version` covers the definitions it holds"),
            );
        }
        Self::parse_doc(path, path, root, nodes, family, src, issues)
    }

    /// Everything one manifest's nodes declare.
    fn parse_doc(
        path: &str,
        dir_rel: &str,
        root: &Path,
        nodes: &[KdlNode],
        family: Option<&str>,
        src: &Source,
        issues: &mut Issues,
    ) -> Self {
        let dir = layout::component(root, dir_rel);
        let mut component = Self::empty(path, dir_rel, root, src);

        let mut fragment_seen = false;
        let mut gated: Vec<(String, Span)> = Vec::new();
        for node in nodes {
            match node.name().value() {
                "on" => {}
                _ => {
                    component.take(node, &[], &dir, src, issues, &mut fragment_seen);
                    continue;
                }
            }
            if kids(node).is_empty() {
                issues.push(
                    Issue::new("`on` gates nothing", src)
                        .at(node.name().span(), "an empty block")
                        .help("a gate with nothing in it says the component does something on that family and then does not; drop it, or move the declaration inside"),
                );
            }
            // A gate this image is not built for is read all the same, so a
            // component published for other families is held to the same checks.
            // What it declared is then dropped.
            let gate = gate_families(node, src, issues);
            gated.extend(
                gate.iter()
                    .map(|name| (name.clone(), node.name().span().into())),
            );
            let keep = family.is_none_or(|want| gate.iter().any(|name| name == want));
            let mark = component.gated();
            for child in kids(node) {
                component.take(child, &gate, &dir, src, issues, &mut fragment_seen);
            }
            if !keep {
                component.ungate(mark);
            }
        }

        check_references(nodes, &component.options, src, issues);
        component.base = base_facts(nodes, src, issues);
        // The `base` node makes a component a base, and an image holds one.
        if component.base.is_some() {
            component.provides.push(Decl {
                name: BASE_IMAGE.to_string(),
                span: Span::default(),
            });
            component.exclusive.push(BASE_IMAGE.to_string());
        }

        // Every batch outside a gate resolved against `targets`, which is only
        // whole now, and the dnf-only nodes held to what they resolved to.
        let targets = component.targets.clone();
        spread(
            &mut component.packages,
            &targets,
            "packages",
            false,
            src,
            issues,
        );
        spread(
            &mut component.groups,
            &targets,
            "package-groups",
            true,
            src,
            issues,
        );

        Self::rest(component, path, root, &dir, &gated, src, issues)
    }

    #[cfg(test)]
    fn parse(
        path: &str,
        dir_rel: &str,
        root: &Path,
        text: String,
        family: Option<&str>,
        issues: &mut Issues,
    ) -> Option<Self> {
        Self::parse_against(
            path,
            dir_rel,
            root,
            text,
            family,
            RepoSchema {
                version: Some(crate::model::image::SCHEMA_VERSION),
                src: &Source::new("repo.kdl", "schema-version 1\n"),
            },
            issues,
        )
    }

    /// One node of a manifest, from the top level or from inside a `family`
    /// gate. `gate` is the families that gate names, and is empty for a node
    /// outside one.
    fn take(
        &mut self,
        node: &KdlNode,
        gate: &[String],
        dir: &Path,
        src: &Source,
        issues: &mut Issues,
        fragment_seen: &mut bool,
    ) {
        let component = self;
        let path = component.path.clone();
        {
            match node.name().value() {
                "description" => {
                    if component.description.is_empty() {
                        component.description = string_arg(node).unwrap_or_default().to_string();
                    }
                }
                "targets" => component.parse_targets(node, src, issues),
                kind @ ("provides" | "requires" | "after" | "removes") => {
                    component.parse_capabilities(kind, node, src, issues)
                }
                "imports" => component.parse_imports(node, src, issues),
                "overrides" => component.parse_overrides(node, src, issues),
                "mode" => component.parse_mode(node, dir, src, issues),
                kind @ ("secret" | "arg") => {
                    for name in string_args(node) {
                        let decl = Decl {
                            name: name.to_string(),
                            span: node.name().span().into(),
                        };
                        if kind == "secret" {
                            component.secrets.push(decl);
                        } else {
                            component.args.push(decl);
                        }
                    }
                }
                "helpers" => component.parse_helpers(node, dir, src, issues),
                "allow-verify" => component.parse_allow_verify(node, src, issues),
                "refuses" => component.parse_refuses(node, src, issues),
                "collects" => component.parse_collects(node, src, issues),
                "contributes" => component.parse_contributes(node, dir, src, issues),
                "fragment" => {
                    if *fragment_seen {
                        return;
                    }
                    *fragment_seen = true;
                    if component.fragment.is_none() {
                        issues.push(
                            Issue::new(
                                format!("`{path}` declares `fragment` but ships no Containerfile.inc"),
                                src,
                            )
                            .at(node.name().span(), "nothing to place")
                            .help("shipping the file is what adds a fragment; this node only says where it goes"),
                        );
                    }
                    component.parse_fragment(node, src, issues);
                }
                "option" => {
                    if let Some(opt) = options::parse_option(node, src, issues) {
                        // A duplicate is a schema issue; keeping the first leaves the read-outs at one.
                        if !component.options.iter().any(|o| o.name == opt.name) {
                            component.options.push(opt);
                        }
                    }
                }
                "network" => {
                    if string_arg(node) == Some("scripts") {
                        component.network = Some(node.name().span().into());
                    }
                }
                "asset" => {
                    if let Some(pin) = asset::parse(node, src, issues) {
                        if !component.assets.iter().any(|a| a.name == pin.name) {
                            component.assets.push(pin);
                        }
                    }
                }
                "variant" => {
                    if let Some(variant) = options::parse_variant(node) {
                        if !component.variants.iter().any(|v| v.name == variant.name) {
                            component.variants.push(variant);
                        }
                    }
                }
                "key" => match parse_key(node, src, issues) {
                    Some(key) => {
                        if !component.keys.iter().any(|k| k.kind == key.kind) {
                            component.keys.push(key);
                        }
                    }
                    None => component
                        .refused_keys
                        .extend(string_arg(node).map(key_capability)),
                },
                "packages" => component.parse_packages(node, gate, src, issues),
                "package-groups" => component.parse_package_groups(node, gate, src, issues),
                "copr" => component.parse_copr(node, gate, src, issues),
                "satisfies" => component.satisfies.extend(coverages(node, src, issues)),
                _ => {}
            }
        }
    }

    /// What the gated nodes have contributed so far.
    fn gated(&self) -> Gated {
        Gated {
            coprs: self.coprs.len(),
            requires: self.requires.len(),
            imports: self.imports.len(),
            after: self.after.len(),
            satisfies: self.satisfies.len(),
        }
    }

    /// Everything one gate contributed, dropped: the block was read for its
    /// diagnostics and this image is not built for the families it names.
    fn ungate(&mut self, mark: Gated) {
        self.coprs.truncate(mark.coprs);
        self.requires.truncate(mark.requires);
        self.imports.truncate(mark.imports);
        self.after.truncate(mark.after);
        self.satisfies.truncate(mark.satisfies);
    }

    fn rest(
        mut component: Self,
        path: &str,
        root: &Path,
        dir: &Path,
        gated: &[(String, Span)],
        src: &Source,
        issues: &mut Issues,
    ) -> Self {
        // A key's public half witnesses `<kind>-key`.
        for key in &component.keys {
            let name = key_capability(&key.kind);
            if !component.provides.iter().any(|d| d.name == name) {
                component.provides.push(Decl {
                    name: name.clone(),
                    span: key.span,
                });
            }
            if !component.files.iter().any(|f| f.name == name) {
                component.files.push(Located {
                    name,
                    file: key.public.clone(),
                    build_only: false,
                });
            }
        }

        for key in &component.keys {
            let file = layout::public_key(root, &key.public);
            if !layout::nonempty(&file) {
                issues.push(
                    Issue::new(
                        format!("`{}` has no public half for its {} key", path, key.kind),
                        src,
                    )
                    .at(key.span, format!("{} is missing or empty", file.display()))
                    .help(format!("run `tect create key {}`", key.kind)),
                );
            }
        }

        // An `enablerepo` naming a COPR is a reference to a `copr` node, which
        // may be read after it: the selector is derived once both are in.
        let coprs: Vec<(String, String)> = component
            .coprs
            .iter()
            .map(|copr| (copr.name(), copr.selector()))
            .collect();
        // COPR is Fedora's: the adapter enables one on a Fedora base alone, so
        // anywhere else it would be skipped and its installs would fail.
        let targets = component.targets.clone();
        for copr in &component.coprs {
            let reach: Vec<&str> = match (copr.family.is_empty(), targets.is_empty()) {
                (false, _) => copr.family.iter().map(String::as_str).collect(),
                (true, false) => targets.iter().map(String::as_str).collect(),
                (true, true) => FAMILIES.to_vec(),
            };
            if let Some(stray) = reach.iter().find(|family| **family != "fedora") {
                issues.push(
                    Issue::new(
                        format!("`copr` is Fedora's, and this one covers `{stray}`"),
                        src,
                    )
                    .at(copr.span, "no COPR is enabled on this family")
                    .help("declare it inside `on \"fedora\"`"),
                );
            }
        }
        for batch in component.packages.iter_mut().chain(&mut component.groups) {
            let Some(named) = batch.enablerepo.as_deref().filter(|v| v.contains('/')) else {
                continue;
            };
            if batch.family != "fedora" {
                issues.push(
                    Issue::new(
                        format!(
                            "`enablerepo` names COPR `{named}`, and this batch covers `{}`",
                            batch.family
                        ),
                        src,
                    )
                    .at(batch.span, "no COPR is enabled on this family")
                    .help("declare the batch inside `on \"fedora\"`"),
                );
                batch.enablerepo = None;
                continue;
            }
            match coprs.iter().find(|(name, _)| name == named) {
                Some((_, selector)) => batch.enablerepo = Some(selector.clone()),
                None => issues.push(
                    Issue::new(
                        format!("`enablerepo` names COPR `{named}`, which is not declared here"),
                        src,
                    )
                    .at(batch.span, "no `copr` for it")
                    .help(format!("add `copr \"{named}\"`; a repository is reachable because the component says so, not because an install names it")),
                ),
            }
        }

        if !component.standard_layer {
            let dropped = component
                .secrets
                .iter()
                .map(|d| ("secret", d.name.as_str(), d.span))
                .chain(
                    component
                        .args
                        .iter()
                        .map(|d| ("arg", d.name.as_str(), d.span)),
                )
                .chain(
                    component
                        .options
                        .iter()
                        .map(|o| ("option", o.name.as_str(), o.span)),
                )
                .chain(
                    component
                        .assets
                        .iter()
                        .map(|a| ("asset", a.name.as_str(), a.span)),
                )
                .chain(
                    component
                        .modes
                        .iter()
                        .map(|m| ("mode", m.path.as_str(), m.span)),
                )
                .chain(
                    std::iter::zip(&coprs, &component.coprs)
                        .map(|((name, _), copr)| ("copr", name.as_str(), copr.span)),
                );
            for (kind, name, span) in dropped {
                issues.push(
                    Issue::new(
                        format!(
                            "`{path}` declares `{kind} \"{name}\"` with no standard layer to carry it"
                        ),
                        src,
                    )
                    .at(span, "nowhere to land")
                    .help("`standard-layer #false` makes the fragment the whole layer, so it has to carry this itself; drop one or the other"),
                );
            }
        }

        for (gated, taken_on) in layout::FAMILY_DIRS {
            if component.targets.is_empty()
                || !dir.join(gated).is_dir()
                || taken_on
                    .iter()
                    .any(|family| component.targets.iter().any(|claimed| claimed == family))
            {
                continue;
            }
            let named = taken_on.join("` or `");
            issues.push(
                Issue::new(
                    format!("`{path}` ships a `{gated}/` directory and targets no family it is taken on"),
                    src,
                )
                .help(format!(
                    "nothing would ever read it, since the directory is taken on `{named}` and \
                     no image on either may enable this component: widen `targets`, or drop the \
                     directory"
                )),
            );
        }
        for (family, span) in gated {
            if component.targets.is_empty()
                || component.targets.iter().any(|claimed| claimed == family)
            {
                continue;
            }
            issues.push(
                Issue::new(
                    format!("`{path}` gates on `{family}` and does not support it"),
                    src,
                )
                .at(*span, "no image reaches this block")
                .help(format!(
                    "a gate narrows what a component already builds on: add `{family}` to \
                         `targets`, or drop the block"
                )),
            );
        }

        crate::provenance::check_fetch(&component, dir, issues);

        component
    }

    /// `targets "fedora"` The families this repository knows how to build on.
    fn parse_targets(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        for family in string_args(node) {
            if !FAMILIES.contains(&family) {
                issues.push(
                    Issue::new(format!("unknown base family `{family}`"), src)
                        .at(node.name().span(), "not a family this repository builds on")
                        .help(format!("known families: {}", FAMILIES.join(", "))),
                );
            }
            self.targets.push(family.to_string());
        }
    }

    /// Reads the names of `provides`, `requires`, `after` and `removes`. Each
    /// node takes the line form and the block form.
    fn parse_capabilities(
        &mut self,
        kind: &str,
        node: &KdlNode,
        src: &Source,
        issues: &mut Issues,
    ) {
        for named in names(node) {
            let decl = Decl {
                name: named.name.to_string(),
                span: named.span,
            };
            check_capability(&decl.name, decl.span, src, issues);
            if kind == "provides" {
                if decl.name == BASE_IMAGE {
                    issues.push(
                        Issue::new("`base-image` is what a `base` node provides", src)
                            .at(decl.span, "declared by hand")
                            .help("a base declares `base \"<image reference>\"`, which provides `base-image`"),
                    );
                }
                self.locate(named.node, &decl, src, issues);
                if flag(named.node, "exclusive") {
                    self.exclusive.push(decl.name.clone());
                }
            }
            match kind {
                "provides" => self.provides.push(decl),
                "requires" => self.requires.push(decl),
                "removes" => self.removes.push(decl),
                _ => self.after.push(decl),
            }
        }
    }

    /// Reads `imports "a" "b"` and `imports { "a" { name "value" } }`. Only the
    /// block form sets options, because each path's own block holds the
    /// options of that component.
    fn parse_imports(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        for named in names(node) {
            let path = named.name;
            // A path that reads an option is checked against components/ once
            // the value is known.
            if !path.contains('$') && !crate::model::image::is_local_path(path) {
                issues.push(
                    Issue::new(format!("`{path}` is not a component path"), src)
                        .at(named.span, "not a path under components/")
                        .help("a path is segments of lowercase letters, digits and dashes, joined by `/`"),
                );
                continue;
            }
            let options = named
                .kids
                .iter()
                .map(|kid| (kid.name().value().to_string(), options::args(kid)))
                .collect();
            self.imports.push(Import {
                path: path.to_string(),
                options,
                span: named.span,
            });
        }
    }

    /// Records the witness of one provided name. `build-only` describes the
    /// witness. `node` holds the properties of that name.
    fn locate(&mut self, node: &KdlNode, decl: &Decl, src: &Source, issues: &mut Issues) {
        let build_only = flag(node, "build-only");
        let Some(witness) = prop(node, "witness") else {
            if build_only {
                issues.push(
                    Issue::new("`build-only` describes a `witness`, and none is given", src)
                        .at(decl.span, "no `witness`")
                        .help("`provides \"x\" witness=\"/usr/libexec/x\" build-only=#true`"),
                );
            }
            return;
        };
        check_path(witness, decl.span, src, issues);
        self.files.push(Located {
            name: decl.name.clone(),
            file: witness.to_string(),
            build_only,
        });
    }

    /// `overrides "/etc/x"`: a path this component's overlay replaces deliberately.
    fn parse_overrides(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        for path in string_args(node) {
            if !path.starts_with('/') {
                issues.push(
                    Issue::new(format!("`{path}` is not an absolute path"), src)
                        .at(node.name().span(), "an exact path in the image"),
                );
            }
            self.overrides.push(Decl {
                name: path.to_string(),
                span: node.name().span().into(),
            });
        }
    }

    /// `mode "/etc/example.conf" "0644"` A chmod mode for one shipped overlay
    /// file. The path stays separate from `provides-file`, which is a contract.
    fn parse_mode(&mut self, node: &KdlNode, dir: &Path, src: &Source, issues: &mut Issues) {
        let args: Vec<_> = node
            .entries()
            .iter()
            .filter(|entry| entry.name().is_none())
            .collect();
        if args.len() != 2 {
            return;
        }
        let Some(path) = args[0].value().as_string() else {
            return;
        };
        let Some(given) = args[1].value().as_string() else {
            return;
        };

        if path == "/"
            || !path.starts_with('/')
            || path
                .split('/')
                .skip(1)
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            issues.push(
                Issue::new(
                    format!("`{path}` is not a normal absolute overlay path"),
                    src,
                )
                .at(args[0].span(), "not a file path under `files/`")
                .help(
                    "name the path exactly as it lands in the image, such as `/etc/example.conf`",
                ),
            );
            return;
        }

        if given.is_empty() || !given.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
            issues.push(
                Issue::new(format!("`{given}` is not an octal file mode"), src)
                    .at(args[1].span(), "use only digits 0 through 7")
                    .help("a chmod file mode such as `0440`"),
            );
            return;
        }
        let mode = match u32::from_str_radix(given, 8) {
            Ok(mode) if mode <= 0o7777 => mode,
            _ => {
                issues.push(
                    Issue::new(format!("file mode `{given}` is out of range"), src)
                        .at(args[1].span(), "greater than 07777"),
                );
                return;
            }
        };

        // The emitter applies a mode after every overlay the family takes.
        let shipped = std::iter::once(dir.join(layout::OVERLAY))
            .chain(
                layout::FAMILY_DIRS
                    .iter()
                    .map(|(name, _)| dir.join(name).join(layout::OVERLAY)),
            )
            .any(|overlay| regular_overlay_file(&overlay, path));
        if !shipped {
            issues.push(
                Issue::new(
                    format!(
                        "`{}` declares a mode for a file it does not ship",
                        self.path
                    ),
                    src,
                )
                .at(
                    args[0].span(),
                    format!("{path} is missing from every `files/`"),
                )
                .help("shipping the overlay file is what makes its mode meaningful"),
            );
            return;
        }
        self.modes.push(FileMode {
            path: path.to_string(),
            mode,
            span: args[0].span().into(),
        });
    }

    /// A refusal is a judgement, so it is worth nothing without its reason:
    /// the rule alone reads as an oversight the next reader will try to fix.
    fn parse_refuses(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let span: Span = node.name().span().into();
        let Some(rule) = string_arg(node).map(str::to_string) else {
            return;
        };
        let because = prop(node, "because").unwrap_or_default().to_string();
        if because.is_empty() {
            issues.push(
                Issue::new(format!("`{rule}` is refused without a reason"), src)
                    .at(span, "no `because`")
                    .help(
                        "a refusal keeps a rule out of remediation for good, so the reason is \
                         what stops the next reader undoing it: `because=\"the base is on \
                         yescrypt, which is stronger\"`",
                    ),
            );
        }
        self.refuses.push(crate::model::component::Refusal {
            rule,
            because,
            span,
        });
    }

    /// `allow-verify "man-page-missing" unit="x.service"` One known diagnostic
    /// accepted on one unit, which is why both halves are required.
    fn parse_allow_verify(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let span: Span = node.name().span().into();
        let class = string_arg(node).map(str::to_string);
        let unit = prop(node, "unit").map(str::to_string);

        let (Some(class), Some(unit)) = (class, unit) else {
            let missing = match (string_arg(node), prop(node, "unit")) {
                (None, _) => "a diagnostic class",
                (_, None) => "unit=, the unit it applies to",
                _ => "both a class and a unit",
            };
            issues.push(
                Issue::new(format!("`allow-verify` needs {missing}"), src)
                    .at(span, "incomplete")
                    .help(
                        "`allow-verify \"man-page-missing\" unit=\"plasmalogin.service\"`, \
                         which accepts one diagnostic on one unit and leaves the rest of the \
                         image checked",
                    ),
            );
            return;
        };

        if !VERIFY_CLASSES.iter().any(|(name, _)| *name == class) {
            issues.push(
                Issue::new(format!("`{class}` is not a verify diagnostic class"), src)
                    .at(span, "not one of the known classes")
                    .help(format!(
                        "known classes: {}. Each is named in full, and `tect validate-image` holds what it stands for",
                        class_names()
                    )),
            );
        } else if let Some(dup) = self
            .verify_exceptions
            .iter()
            .find(|e| e.class == class && e.unit == unit)
        {
            issues.push(
                Issue::new(format!("`{class}` is allowed twice on `{unit}`"), src)
                    .at(dup.span, "first here")
                    .at(span, "and again here"),
            );
        } else {
            self.verify_exceptions
                .push(VerifyException { class, unit, span });
        }
    }

    /// `collects "justfile.inc" into="/usr/share/just/justfile.apps"
    /// priority=500` The filename gathered from every component shipping one, and
    /// where the assembled result goes.
    fn parse_collects(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        if bad_priority(node) {
            return;
        }
        let collected = string_args(node).first().map(|s| s.to_string());
        let into = prop(node, "into");
        match (collected, into, priority(node)) {
            (Some(collected), Some(into), Some(priority)) if into.starts_with('/') => {
                self.collects.push(Collect {
                    file: collected,
                    into: into.to_string(),
                    priority,
                    span: node.name().span().into(),
                })
            }
            (collected, into, priority) => {
                let missing = if collected.is_none() {
                    "the filename it collects"
                } else if into.is_none() {
                    "into=, where the build puts them"
                } else if priority.is_none() {
                    "priority=, where a contribution lands when it names none"
                } else {
                    "an absolute into="
                };
                issues.push(
                    Issue::new(format!("`collects` needs {missing}"), src)
                        .at(node.name().span(), "incomplete")
                        .help("`collects \"justfile.inc\" into=\"/usr/share/just/justfile.apps\" priority=500`"),
                );
            }
        }
    }

    /// `contributes "justfile.inc" priority=900` A file this component ships for
    /// another to collect, so shipping it is what the node is about.
    fn parse_contributes(&mut self, node: &KdlNode, dir: &Path, src: &Source, issues: &mut Issues) {
        if bad_priority(node) {
            return;
        }
        let contributed = string_args(node).first().map(|s| s.to_string());
        match (contributed, priority(node)) {
            (Some(contributed), Some(priority)) => {
                // Every family, since a manifest is read without one: a
                // contribution gated to a family this build is not for is
                // still a file the component ships.
                let anywhere = layout::FAMILY_DIRS
                    .iter()
                    .map(|(gated, _)| dir.join(gated).join(&contributed))
                    .chain(std::iter::once(dir.join(&contributed)))
                    .any(|at| at.is_file());
                if !anywhere {
                    issues.push(
                        Issue::new(
                            format!("`{}` orders a {contributed} it does not ship", self.path),
                            src,
                        )
                        .at(node.name().span(), "nothing to order")
                        .help("shipping the file is what contributes it; this node only says where it lands"),
                    );
                } else if let Some(dup) = self.contributes.iter().find(|c| c.file == contributed) {
                    issues.push(
                        Issue::new(format!("`{contributed}` is ordered twice"), src)
                            .at(dup.span, "first here")
                            .at(node.name().span(), "and again here"),
                    );
                } else {
                    self.contributes.push(Contribution {
                        file: contributed,
                        priority,
                        span: node.name().span().into(),
                    });
                }
            }
            (contributed, _) => {
                let missing = match contributed.is_none() {
                    true => "the filename it contributes",
                    false => "priority=, which is the only thing it declares",
                };
                issues.push(
                    Issue::new(format!("`contributes` needs {missing}"), src)
                        .at(node.name().span(), "incomplete")
                        .help("`contributes \"justfile.inc\" priority=900`, for a component that has to land after the rest"),
                );
            }
        }
    }

    /// `fragment position="after" standard-layer=#false` Defaults are the
    /// additive case: the fragment goes above the generated block and the
    /// block is still emitted.
    fn parse_fragment(&mut self, node: &KdlNode, src: &Source, issues: &mut Issues) {
        let position = prop(node, "position").filter(|p| matches!(*p, "before" | "after" | "tail"));
        self.fragment_position = match position {
            Some("after") => Position::After,
            Some("tail") => Position::Tail,
            _ => Position::Before,
        };
        self.standard_layer = boolean(node, "standard-layer").unwrap_or(true);

        if !self.standard_layer {
            if let Some(span) = position.and_then(|_| prop_span(node, "position")) {
                issues.push(
                    Issue::new(
                        "`position` says nothing without a standard layer",
                        src,
                    )
                    .at(span, "there is nothing to be before or after")
                    .help("`standard-layer #false` makes the fragment the only thing this component emits"),
                );
            }
        }
    }

    /// `helpers "lib/family.sh"` Files mounted from this component into every
    /// standard layer, including layers ordered before this component.
    fn parse_helpers(&mut self, node: &KdlNode, dir: &Path, src: &Source, issues: &mut Issues) {
        for helper in string_args(node) {
            let path = Path::new(helper);
            if path.is_absolute()
                || path
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                issues.push(
                    Issue::new(format!("`{helper}` is not a path inside this component"), src)
                        .at(node.name().span(), "helpers are component content")
                        .help("name a relative file shipped inside this component, such as `lib/family.sh`"),
                );
            } else if !dir.join(path).is_file() {
                issues.push(
                    Issue::new(
                        format!("`{}` declares a helper it does not ship", self.path),
                        src,
                    )
                    .at(node.name().span(), format!("{helper} is missing"))
                    .help("shipping the file is what makes it available to component layers"),
                );
            } else {
                self.helpers.push(Decl {
                    name: helper.to_string(),
                    span: node.name().span().into(),
                });
            }
        }
    }

    /// `packages "htop" "tmux"` The names as positional arguments, taken on the
    /// families of the `family` block holding it, or on every family the component
    /// targets where there is none.
    fn parse_packages(
        &mut self,
        node: &KdlNode,
        gate: &[String],
        src: &Source,
        issues: &mut Issues,
    ) {
        self.packages
            .extend(batch(node, gate, src, issues, "package"));
    }

    /// `copr "owner/project"` The two path segments, which is all a COPR is;
    /// the repository id, its URL and the selector an install enables are
    /// derived from them and never declared.
    fn parse_copr(&mut self, node: &KdlNode, gate: &[String], src: &Source, issues: &mut Issues) {
        let Some(value) = string_arg(node) else {
            return;
        };
        let Some((owner, project)) = copr_parts(value) else {
            issues.push(
                Issue::new(format!("`{value}` is not a COPR owner/project"), src)
                    .at(node.name().span(), "not two usable path segments")
                    .help(format!("`copr \"owner/project\"`; {TOKEN_HELP}")),
            );
            return;
        };
        self.coprs.push(Copr {
            owner: owner.to_string(),
            project: project.to_string(),
            family: gate.to_vec(),
            span: node.name().span().into(),
        });
    }

    /// `package-groups "kde-desktop"` The same shape as `packages`, installed
    /// by the family adapter's group verb.
    fn parse_package_groups(
        &mut self,
        node: &KdlNode,
        gate: &[String],
        src: &Source,
        issues: &mut Issues,
    ) {
        self.groups
            .extend(batch(node, gate, src, issues, "package group"));
    }
}

/// `satisfies { cis-fedora "1.1.1.1" }` Each child names a benchmark and carries
/// the rule IDs the claimant covers. A row with no benchmark name is refused
/// here, so nothing downstream carries a claim it cannot name.
pub(crate) fn coverages(node: &KdlNode, src: &Source, issues: &mut Issues) -> Vec<Coverage> {
    let Some(children) = node.children() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in children.nodes() {
        let benchmark = child.name().value().to_string();
        if benchmark.is_empty() {
            issues.push(
                Issue::new("a benchmark name is required inside `satisfies`", src)
                    .at(child.name().span(), "empty name")
                    .help("`satisfies { cis-fedora \"1.1.1.1\" }`"),
            );
            continue;
        }
        out.push(Coverage {
            benchmark,
            rules: child
                .entries()
                .iter()
                .filter(|entry| entry.name().is_none())
                .filter_map(|entry| entry.value().as_string().map(str::to_string))
                .collect(),
            span: child.name().span().into(),
        });
    }
    out
}

/// The capability a key's public half provides: `secureboot-key` for `key
/// "secureboot"`.
pub(crate) fn key_capability(kind: &str) -> String {
    format!("{kind}-key")
}

/// `key "cosign" { generator "cosign"; public "/etc/..."; private "cosign.key" }`
/// The walker has already held the generator, the profile and the format to
/// their sets, so what is left is the two paths, which are written to.
pub fn parse_key(node: &KdlNode, src: &Source, issues: &mut Issues) -> Option<Key> {
    let kind = string_arg(node)?.to_string();
    let generator = child(node, "generator")?;
    let public = child(node, "public")?;
    let private = child(node, "private")?;

    let path = string_arg(public)?;
    if !path.starts_with('/') || path.split('/').any(|part| part == "..") {
        issues.push(
            Issue::new(format!("`{path}` is not an absolute path in the image"), src)
                .at(public.name().span(), "the public half goes here")
                .help("it is the path the built image ships the public half at, and the component's files/ overlay is what puts it there"),
        );
        return None;
    }

    let name = string_arg(private)?;
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        issues.push(
            Issue::new(format!("`{name}` is not a filename"), src)
                .at(private.name().span(), "the private half is written here")
                .help("the private half is written under keys/private/ and never committed, so it is a plain name"),
        );
        return None;
    }

    let profile = prop(generator, "profile").map(str::to_string);
    let format = prop(public, "format").unwrap_or("pem").to_string();
    if profile.as_deref() == Some("pcr-signing") && format != "pem" {
        issues.push(
            Issue::new(
                format!("`pcr-signing` writes a PEM public key, and `format` says {format}"),
                src,
            )
            .at(public.name().span(), "the public half goes here")
            .help(
                "ukify embeds it and systemd-cryptenroll verifies the policy with it, \
                 and both read PEM",
            ),
        );
        return None;
    }

    // A refused `bits=` drops the key, because a fallback to `BITS` would
    // generate a key size the user did not declare.
    let bits = match prop_span(generator, "bits") {
        Some(_) => ranged(generator, "bits", KEY_SIZES)?,
        None => BITS,
    };
    Some(Key {
        kind,
        generator: string_arg(generator)
            .filter(|g| GENERATORS.contains(g))?
            .to_string(),
        profile,
        bits,
        public: path.to_string(),
        format,
        private: name.to_string(),
        span: node.name().span().into(),
    })
}

/// The span the `satisfies` block takes, which is what a claim written by hand
/// or by a picker replaces. `None` is a manifest declaring none, which the
/// writer appends to instead.
pub fn satisfies_span(kdl: &str) -> Option<Span> {
    let doc: KdlDocument = kdl.parse().ok()?;
    let node = doc
        .nodes()
        .iter()
        .find(|node| node.name().value() == "satisfies")?;
    Some(node.span().into())
}

/// What a manifest says about itself, read without resolving it: what anything
/// asking about a component no image has loaded goes on. Nothing where it does
/// not parse.
#[derive(Default)]
pub struct Summary {
    pub description: String,
    /// Everything it declares as available to another component: `provides`,
    /// `provides-file`, and the public half of every key it declares.
    pub provides: Vec<String>,
    /// Names each provided capability it declares `exclusive=#true`.
    pub exclusive: Vec<String>,
    pub targets: Vec<String>,
    pub requires: Vec<String>,
    /// Lists the paths it imports. A path that reads an option takes the
    /// option's default, because no image entry is known here.
    pub imports: Vec<String>,
    /// The key kinds it declares, which is what an absent one is traced back
    /// to this component by.
    pub keys: Vec<String>,
    /// The build args its layer reads, which is what decides whether a
    /// workflow may run here at all.
    pub args: Vec<String>,
    /// Every benchmark number it claims, over all the benchmarks it names,
    /// since a number resolves against the content.
    pub satisfies: Vec<String>,
}

/// Answers the default of the option `name` that a manifest declares. An
/// option with no default reads as empty. An empty path names no component.
fn default(doc: &KdlDocument, name: &str) -> String {
    doc.nodes()
        .iter()
        .filter(|node| node.name().value() == "option" && string_arg(node) == Some(name))
        .find_map(|node| child(node, "default").and_then(string_arg))
        .unwrap_or_default()
        .to_string()
}

pub fn summary(file: &Path) -> Summary {
    let Some(doc) = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| text.parse::<KdlDocument>().ok())
    else {
        return Summary::default();
    };
    // A gate holds `requires` and `satisfies` among others, and the index
    // answers for the component whatever family it is built on, so a gated
    // declaration counts here the same as one outside.
    let nodes: Vec<&KdlNode> = doc
        .nodes()
        .iter()
        .flat_map(|node| match node.name().value() {
            "on" => kids(node).iter().collect(),
            _ => vec![node],
        })
        .collect();
    let strings = |name: &str| -> Vec<String> {
        nodes
            .iter()
            .filter(|node| node.name().value() == name)
            .flat_map(|node| node.entries())
            .filter(|entry| entry.name().is_none())
            .filter_map(|entry| entry.value().as_string().map(str::to_string))
            .collect()
    };
    let listed = |name: &str| listed(nodes.iter().copied(), name);
    let based = doc.nodes().iter().any(|node| node.name().value() == "base");
    let mut provides = listed("provides");
    provides.extend(strings("key").iter().map(|kind| key_capability(kind)));
    provides.extend(based.then(|| BASE_IMAGE.to_string()));
    let mut exclusive: Vec<String> = nodes
        .iter()
        .filter(|node| node.name().value() == "provides")
        .flat_map(|node| names(node))
        .filter(|named| flag(named.node, "exclusive"))
        .map(|named| named.name.to_string())
        .collect();
    exclusive.extend(based.then(|| BASE_IMAGE.to_string()));
    Summary {
        description: strings("description").join(" "),
        provides,
        exclusive,
        targets: strings("targets"),
        requires: listed("requires"),
        imports: listed("imports")
            .iter()
            .map(|path| resolve_options::interpolate(path, |name| default(&doc, name)))
            .collect(),
        keys: strings("key"),
        args: strings("arg"),
        satisfies: nodes
            .iter()
            .filter(|node| node.name().value() == "satisfies")
            .filter_map(|node| node.children())
            .flat_map(KdlDocument::nodes)
            .flat_map(KdlNode::entries)
            .filter(|entry| entry.name().is_none())
            .filter_map(|entry| entry.value().as_string().map(str::to_string))
            .collect(),
    }
}

/// Reads one component's manifest as a base, or None where it declares no
/// `base`. The reference is written with the default `version`, which
/// is what an image that sets none builds from.
pub fn offered_base(dir: &Path, name: String, owner: Option<String>) -> Option<crate::base::Base> {
    let manifest = std::fs::read_to_string(dir.join(crate::layout::MANIFEST_FILE)).ok()?;
    let doc: KdlDocument = manifest.parse().ok()?;
    let nodes = doc.nodes();
    let one = |want: &str| -> String {
        nodes
            .iter()
            .find(|node| node.name().value() == want)
            .and_then(string_arg)
            .unwrap_or_default()
            .to_string()
    };
    let base = base_block(nodes.iter().find(|node| node.name().value() == "base")?);
    let listed = |want: &str| listed(nodes, want);
    let version = nodes
        .iter()
        .find(|node| node.name().value() == "option" && string_arg(node) == Some("version"))
        .and_then(|node| child(node, "default"))
        .and_then(string_arg)
        .map(str::to_string);
    let image = crate::resolve::options::interpolate(&base.template, |option| match option {
        "version" => version.clone().unwrap_or_default(),
        _ => String::new(),
    });
    Some(crate::base::Base {
        name,
        owner,
        image,
        family: base.family,
        provides: listed("provides"),
        requires: listed("requires"),
        imports: listed("imports"),
        about: one("description"),
        version,
        scap_content: base.scap_content,
        bootloaders: base.bootloaders,
    })
}

/// The schema findings an import has to settle before it copies or references
/// a collection member. Import runs before the component is in the repository,
/// so the ordinary repository load cannot report these yet.
pub(crate) fn validate_manifest(
    file: &Path,
    repo_version: Option<u32>,
    repo_src: &Source,
) -> Issues {
    let mut issues = Issues::default();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(err) => {
            let src = Source::new(file.display().to_string(), String::new());
            issues.push(Issue::new(
                format!("{} cannot be read: {err}", file.display()),
                &src,
            ));
            return issues;
        }
    };
    let src = Source::new(file.display().to_string(), text.clone());
    let doc: KdlDocument = match text.parse() {
        Ok(doc) => doc,
        Err(err) => {
            issues.push(syntax_issue(&err, src.name(), &src));
            return issues;
        }
    };
    check_doc(&doc, &COMPONENT, &src, &mut issues);
    crate::parse::check_file_version(&doc, repo_version, repo_src, &src, &mut issues);
    issues
}

/// Every component on disk that no image lists, held to the schema on its own.
pub fn check_unlisted(list: &List, root: &Path, disk: &Disk, issues: &mut Issues) {
    let listed: BTreeSet<String> = list
        .images
        .iter()
        .flat_map(|image| {
            image
                .entries
                .iter()
                .chain(&image.suppressed)
                .chain(image.base_entry.iter().map(|held| &held.entry))
        })
        .map(|entry| entry.path.clone())
        .collect();

    for dir in disk.components() {
        if listed.contains(dir) {
            continue;
        }
        let file = layout::manifest(root, dir);
        if let Ok(text) = std::fs::read_to_string(&file) {
            Component::parse_against(
                dir,
                dir,
                root,
                text,
                None,
                RepoSchema {
                    version: list.schema_version,
                    src: &list.repo_src,
                },
                issues,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(text: &str) -> Vec<String> {
        let text = format!("schema-version 1\n{text}");
        let doc: KdlDocument = text.parse().expect("valid KDL");
        let src = Source::new("manifest.kdl", text);
        let mut issues = Issues::default();
        check_doc(&doc, &COMPONENT, &src, &mut issues);
        issues.findings()
    }

    /// One manifest parsed as a component on its own: no image lists it, so no
    /// family gate is taken.
    fn parsed(name: &str, text: &str) -> Vec<String> {
        let mut issues = Issues::default();
        let text = format!("schema-version 1\n{text}");
        Component::parse(name, name, Path::new("."), text, None, &mut issues);
        issues.findings()
    }

    #[test]
    fn a_manifest_declares_its_schema_version() {
        let doc: KdlDocument = "packages \"nano\"\n".parse().expect("valid KDL");
        let src = Source::new("manifest.kdl", "packages \"nano\"\n");
        let mut issues = Issues::default();
        check_doc(&doc, &COMPONENT, &src, &mut issues);
        assert_eq!(
            issues.findings(),
            ["this file declares no `schema-version`"]
        );
    }

    /// Omitting the compatibility gate makes an ordinary declaration apply
    /// to every family, just as a component without a manifest does.
    #[test]
    fn no_supports_declaration_means_every_family() {
        let text = "schema-version 1\n\npackages \"nano\"\n";
        let mut issues = Issues::default();
        let component = Component::parse(
            "editor",
            "editor",
            Path::new("."),
            text.to_string(),
            None,
            &mut issues,
        )
        .expect("a component");

        assert!(issues.is_empty(), "{}", issues.plain());
        assert!(component.targets.is_empty());
        assert_eq!(
            component
                .packages
                .iter()
                .map(|batch| batch.family.as_str())
                .collect::<Vec<_>>(),
            FAMILIES
        );
    }

    /// A negative `priority=` is the value that a cast to `u32` wraps.
    #[test]
    fn an_out_of_range_priority_is_reported_once() {
        let found = parsed("absent", "contributes \"other.inc\" priority=-1\n");
        assert_eq!(found, ["`priority` is a number from 0 to 9999"]);
    }

    #[test]
    fn an_out_of_range_key_size_drops_the_key() {
        let text = "key \"secureboot\" {\n\
                    generator \"openssl\" bits=-1\n\
                    public \"/usr/share/secureboot/sb_cert.der\"\n\
                    private \"MOK.priv\"\n\
                    }\n";
        let mut issues = Issues::default();
        let component = Component::parse(
            "keys",
            "keys",
            Path::new("."),
            format!("schema-version 1\n{text}"),
            None,
            &mut issues,
        )
        .expect("a component");
        assert!(component.keys.is_empty());
        assert_eq!(issues.findings(), ["`bits` is a number from 2048 to 16384"]);
    }

    /// A `witness` describes one name, `build-only` describes a `witness`,
    /// and a path is never a name. A clean `witness` is kept beside its name.
    #[test]
    fn a_witness_names_one_capability() {
        let found = parsed(
            "located",
            "provides \"one\" \"two\" witness=\"/usr/bin/one\"\n\
             provides \"three\" build-only=#true\n\
             provides \"four\" witness=\"usr/four\"\n\
             requires \"/usr/bin/five\"\n",
        );
        assert!(
            found
                .iter()
                .any(|m| m.contains("`witness` describes one name, and this `provides` writes 2")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|m| m.contains("describes a `witness`")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|m| m.contains("`usr/four` is not an absolute path")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|m| m.contains("`/usr/bin/five` is a path where")),
            "{found:?}"
        );

        let text = "provides \"helpers\" witness=\"/usr/libexec/helpers.sh\" build-only=#true\n";
        let component = Component::parse(
            "kept",
            "kept",
            Path::new("."),
            text.to_string(),
            None,
            &mut Issues::default(),
        )
        .expect("a component");
        assert_eq!(component.provides[0].name, "helpers");
        assert_eq!(component.files[0].file, "/usr/libexec/helpers.sh");
        assert!(component.files[0].build_only);
        assert_eq!(key_capability("secureboot"), "secureboot-key");
    }

    #[test]
    fn a_block_gives_each_name_its_own_properties() {
        let text = "schema-version 1\nprovides {\n    \"one\"\n    \"two\" witness=\"/usr/bin/two\" exclusive=#true\n}\nrequires { \"three\"; \"four\"; }\n";
        let mut issues = Issues::default();
        let component = Component::parse(
            "block",
            "block",
            Path::new("."),
            text.to_string(),
            None,
            &mut issues,
        )
        .expect("a component");
        assert!(issues.findings().is_empty(), "{:?}", issues.findings());
        let provided: Vec<&str> = component.provides.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(provided, ["one", "two"]);
        assert_eq!(component.files.len(), 1);
        assert_eq!(component.files[0].name, "two");
        assert_eq!(component.exclusive, ["two"]);
        let required: Vec<&str> = component.requires.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(required, ["three", "four"]);
    }

    /// A property on the block node would read as one value for every name.
    /// Two names on one line of the block would leave the second without a
    /// line for its own properties.
    #[test]
    fn a_block_refuses_a_property_on_itself_and_two_names_on_one_line() {
        let found = parsed(
            "block-refused",
            "provides exclusive=#true {\n    \"one\"\n}\nrequires {\n    \"a\" \"b\"\n}\n",
        );
        assert!(
            found.contains(&"`exclusive` describes one name of the `provides` block".to_string()),
            "{found:?}"
        );
        assert!(
            found.contains(&"`b` is a second name on one line of the block".to_string()),
            "{found:?}"
        );
    }

    #[test]
    fn a_list_node_refuses_a_positional_that_is_no_name() {
        let found = parsed(
            "number-refused",
            "requires 5 {\n    \"a\"\n}\npackages {\n    \"htop\" 5\n}\nprovides \"one\" 7\n",
        );
        assert_eq!(
            found,
            [
                "a `requires` name has to be a string",
                "`requires` writes names on its line and a block",
                "`5` is a second name on one line of the block",
                "a `provides` name has to be a string",
            ]
        );
    }

    #[test]
    fn a_base_block_holds_only_the_base_fields() {
        let found = parsed(
            "base-block",
            "base \"example\" { family \"fedora\"; packages \"vim\"; }",
        );
        assert_eq!(found, ["`base` holds no `packages`"]);
    }

    #[test]
    fn a_packages_block_installs_as_one_batch_under_its_enablerepo() {
        let text = "schema-version 1\ntargets \"fedora\"\npackages enablerepo=\"secureblue\" {\n    \"trivalent\"\n    \"trivalent-qt6-ui\"\n}\npackages \"htop\"\n";
        let mut issues = Issues::default();
        let component = Component::parse(
            "batch",
            "batch",
            Path::new("."),
            text.to_string(),
            Some("fedora"),
            &mut issues,
        )
        .expect("a component");
        assert!(issues.findings().is_empty(), "{:?}", issues.findings());
        let batches: Vec<(Vec<String>, Option<String>)> = component
            .packages
            .iter()
            .map(|batch| (batch.packages.clone(), batch.enablerepo.clone()))
            .collect();
        assert_eq!(
            batches,
            [
                (
                    vec!["trivalent".to_string(), "trivalent-qt6-ui".to_string()],
                    Some("secureblue".to_string())
                ),
                (vec!["htop".to_string()], None),
            ]
        );
    }

    /// An `enablerepo` on one name would read as that name's repository.
    /// The install enables the repository for the whole batch.
    #[test]
    fn a_package_name_in_a_block_refuses_a_property() {
        let found = parsed(
            "package-prop",
            "targets \"fedora\"\npackages {\n    \"trivalent\" enablerepo=\"secureblue\"\n}\n",
        );
        assert_eq!(
            found,
            ["`enablerepo` describes the whole batch, and this sits on one name"]
        );
    }

    /// A refusal without its reason reads as an oversight, and the next reader
    /// undoes it. The rule is kept either way: refusing is still the component's
    /// decision, and dropping it would silently let remediation back in.
    #[test]
    fn a_refusal_wants_its_reason_and_is_kept_without_one() {
        let with = parsed(
            "refuses-well",
            r#"
description "refuses one rule"
targets "fedora"
refuses "set_password_hashing_algorithm_logindefs" \
    because="the base is on yescrypt, which is stronger than the SHA512 this asks for"
"#,
        );
        assert!(with.is_empty(), "{with:?}");

        let bare = r#"
description "refuses one rule and says nothing"
targets "fedora"
refuses "grub2_nousb_argument"
"#;
        let without = parsed("refuses-bare", bare);
        assert!(
            without
                .iter()
                .any(|m| m.contains("refused without a reason")
                    && m.contains("grub2_nousb_argument")),
            "{without:?}"
        );
        // And kept: dropping it would let remediation set a rule the component
        // decided to leave alone, which is worse than the missing sentence.
        let mut issues = Issues::default();
        let component = Component::parse(
            "refuses-bare",
            "refuses-bare",
            Path::new("."),
            bare.to_string(),
            None,
            &mut issues,
        )
        .expect("a component missing only a reason still parses");
        assert_eq!(
            component
                .refuses
                .iter()
                .map(|r| r.rule.as_str())
                .collect::<Vec<_>>(),
            ["grub2_nousb_argument"]
        );

        // Two refusals of one rule is a manifest arguing with itself.
        let twice = messages(
            r#"
description "twice"
targets "fedora"
refuses "grub2_nousb_argument" because="one"
refuses "grub2_nousb_argument" because="two"
"#,
        );
        assert!(
            twice.iter().any(|m| m.contains("is refused twice")),
            "{twice:?}"
        );
    }

    #[test]
    fn a_summary_names_only_the_capabilities_declared_exclusive() {
        let dir = std::env::temp_dir().join(format!("tect-exclusive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(layout::MANIFEST_FILE);
        std::fs::write(
            &file,
            "provides \"one\" exclusive=#true\nprovides \"two\"\nkey \"cosign\" { generator \"cosign\" }\n",
        )
        .unwrap();
        let read = summary(&file);
        assert_eq!(read.exclusive, ["one"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The index answers for a component whatever family it is built on, so a
    /// gated declaration counts the same as one outside a gate.
    #[test]
    fn a_summary_reads_a_gated_requirement_and_a_gated_claim() {
        let dir = std::env::temp_dir().join(format!("tect-gated-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(layout::MANIFEST_FILE);
        std::fs::write(
            &file,
            r#"
description "gated"
targets "fedora" "debian"
requires "everywhere"
satisfies { cis-fedora "1.1.1.1" }
on "fedora" {
    requires "on-fedora"
    satisfies { stig "CCI-000199" }
}
"#,
        )
        .unwrap();
        let read = summary(&file);
        assert_eq!(read.requires, ["everywhere", "on-fedora"]);
        assert_eq!(read.satisfies, ["1.1.1.1", "CCI-000199"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// An empty `on` block gates nothing. A `family` names a base's family
    /// inside the `base` block, and a `family` outside it is unknown.
    #[test]
    fn an_on_block_gates_and_a_family_names_a_base() {
        let gate = parsed("gated", "on \"fedora\" { }");
        assert!(gate.contains(&"`on` gates nothing".to_string()), "{gate:?}");
        let stray = parsed("stray", "family \"fedora\"");
        assert_eq!(stray, ["unknown node `family`"]);
        let base = parsed("base", "base \"example\" { family \"arch\"; }");
        assert!(base.is_empty(), "{base:?}");
    }

    /// An architecture has nothing to match until an image declares one.
    #[test]
    fn an_arch_property_is_refused_on_targets_and_on() {
        let found = parsed(
            "arch",
            "targets \"fedora\" arch=\"x86_64\"\non \"fedora\" arch=\"x86_64\" { packages \"vim\" }",
        );
        assert_eq!(
            found,
            [
                "`arch` is not a `targets` property",
                "`arch` is not an `on` property"
            ]
        );
    }

    /// Every shape the golden corpus has no broken fixture for.
    #[test]
    fn the_table_catches_what_the_corpus_does_not() {
        let found = messages(
            r#"
targets "fedora"
description
description "twice"
provides
provides "one" "two" witness="/usr/bin/one"
provides "three" build-only=#true
provides "four" witness="usr/four" build-only="yes" lifetime="long"
requires "/usr/bin/five"
mode "/etc/one"
secret
allow-verify "man-page-missing" unit=1 scope="image"
collects "justfile.inc" into="/etc/one" priority=90000 mode="append"
contributes "justfile.inc" priority=100 mode="append"
fragment "here" position="sideways" standard-layer="no" placement="last"
fragment
packages "one" enablerepo="two" weak=#true
package-groups "one" weak=#true
drives "a truck"
satisfies {
    "" "1.1.1.1"
    cis-fedora
    cis-fedora "1"
    cis-fedora "2"
}
satisfies
family
on "fedora" { }
on "fedora" { targets "debian"; packages }
"#,
        );
        assert_eq!(
            found,
            [
                "`description` needs a string",
                "`description` is declared twice",
                "`provides` needs a capability name",
                "`witness` describes one name, and this `provides` writes 2",
                "`build-only` takes #true or #false",
                "unknown `provides` property `lifetime`",
                "`mode` needs one path and one octal file mode",
                "`secret` needs a name",
                "`unit` must be a string",
                "unknown `allow-verify` property `scope`",
                "`priority` is a number from 0 to 9999",
                "unknown `collects` property `mode`",
                "unknown `contributes` property `mode`",
                "`fragment` takes no arguments",
                "`position` must be \"before\", \"after\" or \"tail\"",
                "`standard-layer` must be #true or #false",
                "unknown fragment property `placement`",
                "`fragment` is declared twice",
                "unknown `packages` property `weak`",
                "unknown `package-groups` property `weak`",
                "unknown node `drives`",
                "`cis-fedora` has no rules listed",
                "benchmark `cis-fedora` is declared twice",
                "`satisfies` is declared twice",
                "unknown node `family`",
                "`targets` is not gated by family",
                "`packages` needs at least one name",
            ]
        );
    }

    #[test]
    fn a_mode_may_name_a_file_only_a_family_ships() {
        let root = std::env::temp_dir().join(format!("tect-mode-family-{}", std::process::id()));
        let component = layout::component(&root, "one");
        let files = component.join("deb").join(layout::OVERLAY).join("etc");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("family.conf"), "deb\n").unwrap();

        let mut issues = Issues::default();
        Component::parse(
            "one",
            "one",
            &root,
            "description \"family mode\"\ntargets \"debian\"\nmode \"/etc/family.conf\" \"0600\"\n"
                .to_string(),
            None,
            &mut issues,
        );
        let found = issues.plain();
        assert!(!found.contains("does not ship"), "{found}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn modes_need_a_regular_overlay_file_and_a_standard_layer() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("tect-mode-{}", std::process::id()));
        let component = layout::component(&root, "one");
        let files = component.join(layout::OVERLAY).join("etc");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("regular.conf"), "regular\n").unwrap();
        symlink("regular.conf", files.join("link.conf")).unwrap();
        std::fs::write(component.join("Containerfile.inc"), "RUN true\n").unwrap();

        let mut issues = Issues::default();
        Component::parse(
            "one",
            "one",
            &root,
            r#"
description "mode checks"
targets "fedora"
mode "/etc/link.conf" "0440"
mode "/etc/regular.conf" "0440"
fragment standard-layer=#false
"#
            .to_string(),
            None,
            &mut issues,
        );
        let found = issues.plain();
        assert!(
            found.contains("declares a mode for a file it does not ship"),
            "{found}"
        );
        assert!(
            found.contains("declares `mode \"/etc/regular.conf\"` with no standard layer"),
            "{found}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A package group is the dnf adapter's verb, so no other family may
    /// declare one at all.
    #[test]
    fn package_groups_are_dnf_only() {
        assert_eq!(
            parsed(
                "groups",
                r#"
description "package groups"
targets "fedora" "debian"
package-groups "kde-desktop"
"#
            ),
            ["`package-groups` is dnf's, and this one covers `debian`"]
        );
    }

    /// COPR is enabled on a Fedora base alone, so a COPR or an `enablerepo`
    /// naming one that reaches `rhel` would emit an install nothing enabled.
    #[test]
    fn a_copr_reaches_fedora_alone() {
        assert_eq!(
            parsed(
                "copr-rhel",
                r#"
description "copr"
targets "rhel"
copr "owner/project"
on "rhel" { packages "thing" enablerepo="owner/project"; }
"#
            ),
            [
                "`copr` is Fedora's, and this one covers `rhel`",
                "`enablerepo` names COPR `owner/project`, and this batch covers `rhel`",
            ]
        );
        assert!(parsed(
            "copr-fedora",
            r#"
description "copr"
targets "fedora" "rhel"
on "fedora" {
    copr "owner/project"
    packages "thing" enablerepo="owner/project"
}
"#
        )
        .is_empty());
        // Ungated beside a deb family: dropped there without a word until the
        // COPR was held to Fedora.
        assert_eq!(
            parsed(
                "copr-deb",
                r#"
description "copr"
targets "fedora" "debian"
copr "owner/project"
"#
            ),
            ["`copr` is Fedora's, and this one covers `debian`"]
        );
    }

    /// Both dnf families take a group and an `enablerepo` ungated.
    #[test]
    fn package_groups_and_enablerepo_reach_both_dnf_families() {
        assert!(parsed(
            "dnf",
            r#"
description "package groups"
targets "fedora" "rhel"
package-groups "kde-desktop"
packages "tailscale" enablerepo="tailscale-stable"
"#
        )
        .is_empty());
    }

    /// A COPR is two path segments and nothing else, so anything the RUN line
    /// or the URL could not carry is refused where it is written.
    #[test]
    fn a_copr_needs_two_usable_path_segments() {
        assert_eq!(
            parsed(
                "coprs",
                r#"
description "coprs"
targets "fedora"
copr "noslash"
copr "/project"
copr "owner/"
copr "owner/project/extra"
copr "owner/pro ject"
"#
            ),
            [
                "`noslash` is not a COPR owner/project",
                "`/project` is not a COPR owner/project",
                "`owner/` is not a COPR owner/project",
                "`owner/project/extra` is not a COPR owner/project",
                "`owner/pro ject` is not a COPR owner/project",
            ]
        );
    }

    /// An `enablerepo` naming a COPR is a reference, so the component has to
    /// declare the repository it reaches into.
    #[test]
    fn an_enablerepo_naming_a_copr_needs_the_declaration() {
        assert_eq!(
            parsed(
                "coprs",
                r#"
description "coprs"
targets "fedora"
copr "owner/project"
packages "thing" enablerepo="other/project"
"#
            ),
            ["`enablerepo` names COPR `other/project`, which is not declared here"]
        );
    }

    /// The gate, on the two nodes it exists for. A block the image is not built
    /// for contributes nothing and is still read.
    #[test]
    fn a_gate_is_taken_on_its_families_and_read_on_every_other() {
        let manifest = r#"schema-version 1

description "gated"
targets "fedora" "debian" "ubuntu"
packages "curl"
on "fedora" {
    after "hardened-malloc"
    copr "secureblue/hardened_malloc"
    satisfies { stig "CCI-000199" }
    packages "vim-enhanced" "BAD NAME"
}
on "debian" "ubuntu" { packages "vim" }
"#;
        let read = |family: Option<&str>| {
            let mut issues = Issues::default();
            let component = Component::parse(
                "gated",
                "gated",
                Path::new("."),
                manifest.to_string(),
                family,
                &mut issues,
            )
            .expect("the manifest parses");
            let packages: Vec<String> = component
                .packages
                .iter()
                .map(|batch| format!("{}:{}", batch.family, batch.packages.join(" ")))
                .collect();
            let said: Vec<String> = issues.findings();
            (
                component.after.len(),
                component.coprs.len(),
                component.satisfies.len(),
                packages,
                said,
            )
        };

        // A Fedora build takes the Fedora gate and nothing else.
        let (after, coprs, satisfies, packages, said) = read(Some("fedora"));
        assert_eq!((after, coprs, satisfies), (1, 1, 1));
        assert_eq!(
            packages,
            [
                "fedora:curl",
                "debian:curl",
                "ubuntu:curl",
                "fedora:vim-enhanced",
                "debian:vim",
                "ubuntu:vim",
            ]
        );
        // The gate is read wherever it is built, so the bad name is reported once.
        assert_eq!(
            said,
            ["package name `BAD NAME` has a character that is not allowed"]
        );

        // A Debian build takes neither the edge, the COPR nor the claim, and
        // reports the same thing about a block it does not build.
        let (after, coprs, satisfies, packages, said) = read(Some("debian"));
        assert_eq!((after, coprs, satisfies), (0, 0, 0));
        assert_eq!(
            packages,
            [
                "fedora:curl",
                "debian:curl",
                "ubuntu:curl",
                "fedora:vim-enhanced",
                "debian:vim",
                "ubuntu:vim",
            ]
        );
        assert_eq!(
            said,
            ["package name `BAD NAME` has a character that is not allowed"]
        );

        // A component no image lists is held to all of it and keeps all of it.
        let (after, coprs, satisfies, _, _) = read(None);
        assert_eq!((after, coprs, satisfies), (1, 1, 1));
    }

    /// A gate or a directory naming a family the component does not support is a
    /// block no image can reach, which is a typo far more often than intent.
    #[test]
    fn a_gate_on_an_unsupported_family_is_refused() {
        assert_eq!(
            parsed(
                "narrow",
                r#"
description "narrow"
targets "fedora"
on "debian" { packages "vim" }
on "redhat" { packages "vim" }
"#
            ),
            [
                "unknown base family `redhat`",
                "`narrow` gates on `debian` and does not support it",
            ]
        );
    }

    /// `targets` and `packages` walk every family the tool recognises, and
    /// `enablerepo` stays dnf-only across every family -- both for a batch that
    /// spread over `targets` and for one a gate placed.
    #[test]
    fn debian_and_ubuntu_are_known_families_and_enablerepo_stays_dnf_only() {
        assert_eq!(
            parsed(
                "known",
                r#"
description "known families and packages"
targets "fedora" "debian" "ubuntu"
packages "curl" enablerepo="rpmfusion"
on "debian" "ubuntu" { packages "curl" enablerepo="backports" }
"#
            ),
            [
                "`enablerepo` is dnf's, and this batch covers `debian`",
                "`enablerepo` is dnf's, and this batch covers `debian`",
            ]
        );
    }

    /// The three nodes a component may repeat under one name, and what they hold.
    #[test]
    fn the_table_holds_the_option_variant_and_asset_grammars() {
        let found = messages(
            r#"
targets "fedora"
option "fonts" type="list" scope="image" {
    default "A"
    default "B"
    describe "the fonts"
}
option "fonts" type="list"
variant "lean" scope="image" {
    set
}
variant "lean"
asset "starship" tracked=#true {
    pin {
        version
        sha256 "abc" from="upstream" algorithm="sha512"
    }
    checksum "abc"
}
asset "starship"
"#,
        );
        assert_eq!(
            found,
            [
                "unknown option property `scope`",
                "`default` is declared twice",
                "unknown node `describe` in an option",
                "option `fonts` is declared twice",
                "unknown variant property `scope`",
                "`set` needs an option name",
                "variant `lean` is declared twice",
                "unknown asset property `tracked`",
                "`version` needs a value",
                "`from` must be asset, sidecar or manual",
                "unknown sha256 property `algorithm`",
                "unknown node `checksum` in an asset",
                "asset `starship` is declared twice",
            ]
        );
    }

    /// A PCR signing public half is PEM whatever `format` says, because that
    /// is what `ukify` embeds and `systemd-cryptenroll` verifies with.
    #[test]
    fn a_pcr_signing_public_half_is_pem() {
        let found = parsed(
            "pcr",
            "description \"x\"\n\ntargets \"fedora\"\n\n\
             key \"pcr\" {\n\
             \x20   generator \"openssl\" profile=\"pcr-signing\" bits=4096\n\
             \x20   public \"/usr/share/secureboot/pcr.pub\" format=\"der\"\n\
             \x20   private \"pcr.priv\"\n\
             }\n",
        );
        assert_eq!(
            found,
            ["`pcr-signing` writes a PEM public key, and `format` says der"]
        );
    }
}
