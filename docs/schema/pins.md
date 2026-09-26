# Where something comes from

Four slots answer it, and every pin in the tree fills the same four: the
locator says where it comes from, the selector which version of it, the
verifier what proves you got that one, and the tracker who keeps the selector
current. One `pin` block carries them, and `asset`, an out-of-tree module and a
collection each hold it. A base carries its locator and selector joined in the
image reference, and `signed` as its verifier.

Every pin has to say how it is kept current: `renovate` for one a bot bumps,
`manual` for one nothing tracks, and why. Exactly one of them, because a pin
that says neither goes stale in silence. A collection has a third answer,
`unpinned`, which is the only one that leaves the content unverified.

Renovate matches `renovate` together with the line directly below it, so the
`version` it rewrites has to sit there with nothing in between.

<!-- schema: pin -->

### `pin`

Where this comes from, which version of it, what proves you got that one, and what keeps the selector current.

*at most one*

| Node | Takes | Meaning |
| --- | --- | --- |
| `manual` | a string, at most one | Why nothing tracks this pin. |
| `unpinned` | a string, at most one | Why this follows a moving ref with no `sha256`, so every fetch takes whatever the ref holds then and nothing checks what arrived. |
| `version` | a string, at most one | The selector: the version, tag or commit this is taken at, which the URL expands and Renovate rewrites. |
| `url` | a string, at most one | The locator: where the content comes from. |
| `path` | a string, at most one | The directory inside the archive the content sits in. |

#### `renovate`

The custom manager Renovate matches to keep the selector current.

*at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `datasource=` | `github-releases`, `github-tags`, `git-refs`, required | Which Renovate datasource the pin is tracked through. |
| `depName=` | a string, required | What that datasource calls the thing being tracked. |
| `extractVersion=` | a string | The pattern Renovate pulls the version out of the tag with. |

#### `sha256`

The verifier: what the fetched content is held to.

*a string, at most one*

| Property | Value | Meaning |
| --- | --- | --- |
| `from=` | `asset`, `sidecar`, `manual` | Where the hash is refreshed from. |

<!-- /schema: pin -->

## Asset pins

An asset is an upstream payload the module fetches during its own layer,
pinned by version and by hash. Its fields reach that layer as
`ASSET_<NAME>_VERSION`, `_URL` and `_SHA256`, with `{version}` already
expanded, so nothing in shell derives a URL.

```kdl
asset "starship" {
    pin {
        renovate datasource="github-releases" depName="starship/starship"
        version "1.26.0"
        url "https://github.com/starship/starship/releases/download/v{version}/starship-x86_64-unknown-linux-musl.tar.gz"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3" from="sidecar"
    }
}
```

<!-- schema: asset -->

### `asset`

A pinned upstream payload the module fetches, reaching the build as ASSET_*.

*a string, one per name, never empty*

Also holds [`pin`](#pin).

<!-- /schema: asset -->

## The import record

`tect copy module` copies a module in and writes a `provenance.kdl` beside
its `module.kdl`, recording which collection it came from, what pinned that
collection, and what the directory hashed to. It is a sibling file and never
part of `module.kdl`: that is the author's file, and rewriting it while copying
would fork it from upstream and break the very comparison the hash exists to
make. The record excludes itself from its own hash, so what `content` names is
the directory except this one file.

```kdl
imported "tectonic-os" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    pin {
        unpinned "the collection this tool is published alongside, followed at its branch head"
        version "main"
        url "https://github.com/tectonic-os/modules/archive/refs/heads/{version}.tar.gz"
    }
}
```

`plan.json` carries the same hash per module, so `verify` fails on a module
edited without regenerating. `tect check` names a module whose content no
longer matches its record. That is not an error: forking an imported module is
legitimate, and what the record buys is that the fork is visible rather than
silent.

<!-- schema: imported -->

### `imported`

Where this module was copied from, and what its content hashed to then. Written by `tect copy module`; the module's author does not maintain it.

*a string, exactly one*

Also holds [`pin`](#pin).

| Node | Takes | Meaning |
| --- | --- | --- |
| `content` | a string, exactly one | What the module directory hashed to when it was imported, every file in it except this one. |

<!-- /schema: imported -->
