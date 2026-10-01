//! Pure functions of one resolved plan, and of the schema itself.

pub mod containerfile;
pub mod coverage;
pub mod finalize;
pub mod graph;
pub mod module_build;
pub mod plan;
pub mod recipe;
pub mod sbom;
pub mod schema_md;
pub mod seed;
pub mod summary;
pub mod why;
pub mod workflows;

/// Shell libraries every generated build context carries.
pub(crate) const LIBRARIES: &[(&str, &str)] = &[
    (
        "apparmor-helpers.sh",
        include_str!("../../assets/lib/apparmor-helpers.sh"),
    ),
    (
        "dkms-helpers.sh",
        include_str!("../../assets/lib/dkms-helpers.sh"),
    ),
    (
        "fetch-helpers.sh",
        include_str!("../../assets/lib/fetch-helpers.sh"),
    ),
    (
        "kernel-helpers.sh",
        include_str!("../../assets/lib/kernel-helpers.sh"),
    ),
    (
        "selinux-helpers.sh",
        include_str!("../../assets/lib/selinux-helpers.sh"),
    ),
    (
        "sign-helpers.sh",
        include_str!("../../assets/lib/sign-helpers.sh"),
    ),
    (
        "wrap-helpers.sh",
        include_str!("../../assets/lib/wrap-helpers.sh"),
    ),
];

/// A script every repository runs. The workflows, the other scripts and
/// `tect vm` call it where `layout::script` finds it.
pub(crate) struct Script {
    pub name: &'static str,
    /// The line that the `create scripts` picker shows beside the name.
    pub about: &'static str,
    pub body: &'static str,
}

pub(crate) const SCRIPTS: &[Script] = &[
    Script {
        name: "lint.sh",
        about: "the repository's lint, as CI runs it",
        body: include_str!("../../assets/scripts/lint.sh"),
    },
    Script {
        name: "tect.sh",
        about: "fetches and runs the tect release that repo.kdl pins",
        body: include_str!("../../assets/scripts/tect.sh"),
    },
    Script {
        name: "smoke.sh",
        about: "boots a built image under qemu and waits for ssh",
        body: include_str!("../../assets/scripts/smoke.sh"),
    },
    Script {
        name: "vm.sh",
        about: "builds a disk image and boots it",
        body: include_str!("../../assets/scripts/vm.sh"),
    },
];

/// The text with each `scripts/<name>` it names moved to where that script
/// lives in this repository. The shipped text names every script in `scripts/`.
pub(crate) fn located(root: &std::path::Path, text: &str) -> String {
    SCRIPTS.iter().fold(text.to_string(), |text, script| {
        text.replace(
            &format!("{}/{}", crate::layout::SCRIPTS, script.name),
            &crate::layout::script(root, script.name),
        )
    })
}

pub enum Part {
    Heading(String),
    Text(String),
    Table(Table),
}

/// One table of a read-out, in whatever the caller renders tables with: a
/// terminal draws it, a redirect gets the same data as markdown. Owned, and
/// knowing nothing of the widget: that dependency runs from the widget to here.
pub struct Table {
    pub title: String,
    pub header: &'static [&'static str],
    /// Each row's cells, and whether what the row says is a defect.
    pub rows: Vec<(Vec<String>, bool)>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn repository_lint_reads_generated_libraries() {
        let lint = super::SCRIPTS
            .iter()
            .find(|script| script.name == "lint.sh")
            .unwrap()
            .body;
        assert!(lint.contains("roots=(generated/lib modules)"));
    }
}
