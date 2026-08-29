//! Pure functions of one resolved plan, and of the schema itself.

pub mod containerfile;
pub mod coverage;
pub mod finalize;
pub mod graph;
pub mod json;
pub mod module_build;
pub mod plan;
pub mod sbom;
pub mod schema_md;
pub mod seed;
pub mod summary;
pub mod why;
pub mod workflows;

/// Shell libraries every generated build context carries.
pub(crate) const LIBRARIES: &[(&str, &str)] = &[
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
        "repo-helpers.sh",
        include_str!("../../assets/lib/repo-helpers.sh"),
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

/// The scripts every repository runs, written in place rather than under
/// `generated/`: the generated workflows, the lint script and `tect vm` call
/// them at their fixed paths.
pub(crate) const SCRIPTS: &[(&str, &str)] = &[
    (
        "scripts/lint.sh",
        include_str!("../../assets/scripts/lint.sh"),
    ),
    (
        "scripts/render-iso-config.sh",
        include_str!("../../assets/scripts/render-iso-config.sh"),
    ),
    (
        "scripts/tect.sh",
        include_str!("../../assets/scripts/tect.sh"),
    ),
    ("scripts/vm.sh", include_str!("../../assets/scripts/vm.sh")),
];

#[cfg(test)]
mod tests {
    #[test]
    fn repository_lint_reads_generated_libraries() {
        let lint = super::SCRIPTS
            .iter()
            .find(|(path, _)| *path == "scripts/lint.sh")
            .unwrap()
            .1;
        assert!(lint.contains("find scripts generated/lib modules"));
    }
}

pub enum Part {
    Heading(String),
    Text(String),
    Table(Table),
}

/// One table of a read-out, in whatever the caller renders tables with: a
/// terminal draws it, a redirect gets the same data as markdown. Owned, and
/// knowing nothing of the widget: that dependency runs from `ui/` to here.
pub struct Table {
    pub title: String,
    pub header: &'static [&'static str],
    /// Each row's cells, and whether what the row says is a defect.
    pub rows: Vec<(Vec<String>, bool)>,
}
