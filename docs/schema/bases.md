# The base catalog

`tect create image` offers the bases it knows: what family each belongs to and
what it already ships, so an image scaffolded on one lists no module the base
already carries. The tool compiles one `bases.kdl` into the binary as a
fallback, and the release ships that same file for runtime replacement: a
runtime `bases.kdl` beside the binary, when present, replaces the compiled-in
catalog entirely rather than merging with it; a present but unreadable or
malformed runtime file is diagnosed by its exact path instead of silently
falling back to the compiled-in one. A collection then extends the selected
catalog with a `bases.kdl` at its root, beside the modules.

```kdl
base "ghcr.io/ublue-os/bazzite:stable" {
    about "KDE, gaming and hardware support over kinoite-main"
    family "fedora"
    provides "rechunking" "flatpak"
    signed #true
}

capability "ssh" "/usr/sbin/sshd"
capability "rechunking"
```

A name is witnessed by a path: a module's `provides "<name>" file="<path>"`,
then a `capability` row, then `/usr/bin/<name>` or `/usr/sbin/<name>`. A row
with no path names a capability nothing witnesses. `validate-image` checks the
finished image for every name the base claims and every name a module or a row
locates, and `base-sig-probe` reads the same witnesses when it measures a base.
`luks-initramfs` is the exception because its witness is inside an archive:
declaring it opts a target into `lsinitrd` validation during the build, and
omitting it keeps root encryption unavailable in the installer. A custom base
or module should declare it only when its initramfs is intended to unlock LUKS;
`tect build` then proves the binary is present before installation can offer it.

A collection entry wins over the selected catalog's entry of the same
reference, which is how a stale one is corrected without a tool release, and
`check` names the collection wherever the two differ. A base two collections
describe is an error, the way a collection declared twice is. Nothing is
fetched to read one: a collection that is not on this machine already extends
nothing, and the selected catalog is what the picker offers.

**No row carries a digest, and that is deliberate.** A base reference joins the
locator and the selector, so a digest could be written here — but a digest in
the catalog is a digest only a tool release can roll, and a row that goes stale
between releases is one nobody can fix without waiting for the next one. Worse,
a registry that retires the digest leaves a catalog pin that no longer resolves
at all. So the catalog offers the tag, and the digest belongs in the image file
built on it, where the repository taking the risk takes the decision:

```kdl
image {
    name "Ubuntu"

    base "docker.io/library/ubuntu:26.04@sha256:889d056d5c6c…" {
        family "ubuntu"
        requires "bootc-base"
        signed #false
    }
}
```

The digest is matched off before a reference is looked up, so a pinned base is
still the catalog's base: `tect create image --base` writes the same `family`,
`provides` and `requires` for both spellings, and Renovate's image-file manager
bumps whichever half is written. A row carrying `signed #false` is where this
matters most: with nothing to attribute a signature to, the digest is the
strongest trust root on offer.

`tect build` records the base either way, but it records two different things:
a tag is resolved against the registry and the record says what answered, while
a declared digest is already the answer and is recorded verbatim. Recording is
not pinning — the record says what one build got, and only a digest in the
image file says what the next one will.

<!-- schema: bases -->

| Node | Takes | Meaning |
| --- | --- | --- |
| `capability` | one or more strings, one per name | Where a capability's presence is read: the path that witnesses it, or none for a capability nothing witnesses. |

### `base`

One base a collection describes, named by the reference an image builds on.

*a string, one per name*

| Node | Takes | Meaning |
| --- | --- | --- |
| `about` | a string, exactly one | The line a base picker shows beside the reference. |
| `family` | a string, exactly one | The family an image built on this base declares, matched against every module's `supports`. |
| `provides` | one or more strings | Capabilities this base already ships, by name, written into every image scaffolded on it. |
| `requires` | one or more strings | Capabilities this base is unusable without, which an enabled module must provide. |
| `signed` | `#true` or `#false`, at most one | Whether this base publishes a cosign signature, which a scaffolded image records. |
| `scap-content` | a string, at most one | The SSG datastream this base is measured against, named as a bare filename. |
| `bootloader` | one or more strings, exactly one | The bootloaders an image on this base can install, the one it boots by default first. |

<!-- /schema: bases -->
