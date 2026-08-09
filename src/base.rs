//! The bases the tool knows: what family each belongs to, and what it already
//! ships. Reference data, not a module. A base that is not in here is not an
//! error; it is a base nothing can describe, so `check` reports what is
//! unsatisfied.

/// One known base.
pub struct Base {
    /// The full image reference, written verbatim into `base`.
    pub image: &'static str,
    pub family: &'static str,
    /// Capabilities the upstream image already ships, which suppress a module
    /// providing only these.
    pub provides: &'static [&'static str],
    pub provides_files: &'static [&'static str],
    /// What a person needs to see to pick between this and the rest.
    pub about: &'static str,
}

pub const CATALOG: &[Base] = &[
    Base {
        image: "quay.io/fedora/fedora-bootc:44",
        family: "fedora",
        provides: &["rechunking", "initramfs-generation", "mac-policy"],
        provides_files: &[],
        about: "Fedora 44, nothing above the base system",
    },
    Base {
        image: "ghcr.io/ublue-os/bazzite:stable",
        family: "fedora",
        provides: &["rechunking", "flatpak"],
        provides_files: &["/usr/bin/flatpak"],
        about: "KDE, gaming and hardware support over kinoite-main",
    },
    Base {
        image: "ghcr.io/ublue-os/aurora:stable",
        family: "fedora",
        provides: &["rechunking", "flatpak"],
        provides_files: &["/usr/bin/flatpak"],
        about: "KDE developer workstation over kinoite-main",
    },
    Base {
        image: "ghcr.io/ublue-os/bluefin:stable",
        family: "fedora",
        provides: &["rechunking", "flatpak"],
        provides_files: &["/usr/bin/flatpak"],
        about: "GNOME developer workstation over silverblue-main",
    },
    Base {
        image: "ghcr.io/ublue-os/kinoite-main:44",
        family: "fedora",
        provides: &["rechunking", "flatpak"],
        provides_files: &["/usr/bin/flatpak"],
        about: "Fedora Kinoite with the ublue additions, what bazzite builds on",
    },
];

/// What an image that has chosen nothing builds on.
pub const DEFAULT: &Base = &CATALOG[0];

pub fn find(image: &str) -> Option<&'static Base> {
    CATALOG.iter().find(|base| base.image == image)
}
