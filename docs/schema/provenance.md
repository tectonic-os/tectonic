<a id="provenance.kdl"></a>

# `provenance.kdl`

Location: `modules/<module-name>/provenance.kdl`, beside the `module.kdl` of a copied module.

When `tect copy module` copies a module into this repository, it writes this record beside the module. The record keeps where the module came from and what its content hashed to, so `tect check` can show whether the copy has changed since.

```kdl
imported "tectonic-os" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`imported`](#imported) (required) | *string*, then {&nbsp;[fields](#imported-fields)&nbsp;} | Where this module was copied from, and what its content hashed to then. |

<a id="imported"></a>

## `imported` (required)

Where this module was copied from, and what its content hashed to then.

```kdl
imported "tectonic-os" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

Accepts: *string*, then {&nbsp;[fields](#imported-fields)&nbsp;}

<a id="imported-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`content`](#imported-content) (required) | *string* | The hash that the module directory had at import, which excludes `provenance.kdl`. |
| [`pin`](#imported-pin) (optional) | {&nbsp;[fields](#imported-pin-fields)&nbsp;} | Where content comes from, which version it is, what verifies it, and what keeps the version current. |

> [!NOTE]
> `tect copy module` writes this record to `provenance.kdl` beside the module's `module.kdl`. The module's author does not maintain it.
>
> The record stays out of `module.kdl`. A rewrite of the author's file would fork it from upstream and break the hash comparison.
>
> `plan.json` carries the same hash for each module, so `tect verify` fails on a module edited without a new `tect generate`.
>
> If a module's content no longer matches its record, then `tect check` names the module. The mismatch is an error only under `audit { enforce #true }`, because a fork of an imported module is otherwise allowed.

<a id="imported-content"></a>

### `content` (required)

The hash that the module directory had at import, which excludes `provenance.kdl`.

```kdl
imported "tectonic-os" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

Accepts: *string*

<a id="imported-pin"></a>

### `pin` (optional)

A pin fixes where a download comes from and which version it is, and says how the version stays current. Unless the pin is `unpinned`, `tect` verifies each download against the pinned hash, so a changed upstream file is caught.

```kdl
imported "tectonic-os" {
    pin {
        renovate datasource="github-tags" depName="owner/repo"
        version "v1.0.0"
        url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    }
}
```

Accepts: {&nbsp;[fields](#imported-pin-fields)&nbsp;}

A pin holds exactly one of these, so every pin says how it stays current:

- `renovate`, if Renovate keeps the version current;
- `manual`, if nothing tracks the pin;
- `unpinned`, if the pin follows a moving ref with no `sha256`.

<a id="imported-pin-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`renovate`](#imported-pin-renovate) (optional) |  | The Renovate custom manager that keeps `version` current. |
| [`manual`](#imported-pin-manual) (optional) | *string* | Why nothing tracks this pin. |
| [`unpinned`](#imported-pin-unpinned) (optional) | *string* | Why this pin follows a moving ref with no `sha256`. |
| [`version`](#imported-pin-version) (required) | *string* | The version, tag or commit that `url` expands and Renovate rewrites. |
| [`url`](#imported-pin-url) (required) | *string* | Where the content comes from. |
| [`sha256`](#imported-pin-sha256) (optional) | *string* | The hash the fetched content must match. |
| [`path`](#imported-pin-path) (optional) | *string* | The directory inside the archive that holds the content. |

> [!NOTE]
> An `asset`, an out-of-tree module and a collection each hold a `pin`.
>
> A base holds no `pin`. Its image reference holds the location and the version, and `signed` records whether the base publishes a cosign signature.
>
> Renovate matches `renovate` together with the line directly below it. Put `version` on that line.

<a id="imported-pin-renovate"></a>

#### `renovate` (optional)

The Renovate custom manager that keeps `version` current.

```kdl
imported "tectonic-os" {
    pin {
        renovate datasource="github-tags" depName="owner/repo"
    }
}
```

| Property | Accepts | Description |
| --- | --- | --- |
| `datasource=` (required) | `github-releases` or `github-tags` or `git-refs` | The Renovate datasource that tracks the pin. |
| `depName=` (required) | *string* | The name the datasource knows the dependency by. |
| `extractVersion=` (optional) | *string* | The pattern that extracts the version from a tag. |

<a id="imported-pin-manual"></a>

#### `manual` (optional)

Why nothing tracks this pin.

```kdl
imported "tectonic-os" {
    pin {
        manual "upstream publishes no releases"
    }
}
```

Accepts: *string*

<a id="imported-pin-unpinned"></a>

#### `unpinned` (optional)

Why this pin follows a moving ref with no `sha256`.

```kdl
imported "tectonic-os" {
    pin {
        unpinned "followed at its branch head"
    }
}
```

Accepts: *string*

> [!NOTE]
> Every fetch takes what the ref holds at that moment, and nothing checks what arrived.
>
> Only a collection's pin takes `unpinned`. The build runs an out-of-tree module as root, so the pin of an out-of-tree module needs a `sha256`.
>
> `unpinned` does not combine with `sha256`, because a moving ref breaks the hash.

<a id="imported-pin-version"></a>

#### `version` (required)

The version, tag or commit that `url` expands and Renovate rewrites.

```kdl
imported "tectonic-os" {
    pin {
        version "v1.0.0"
    }
}
```

Accepts: *string*

<a id="imported-pin-url"></a>

#### `url` (required)

Where the content comes from.

```kdl
imported "tectonic-os" {
    pin {
        url "https://github.com/owner/repo/archive/refs/tags/{version}.tar.gz"
    }
}
```

Accepts: *string*

<a id="imported-pin-sha256"></a>

#### `sha256` (optional)

The hash the fetched content must match.

```kdl
imported "tectonic-os" {
    pin {
        sha256 "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
    }
}
```

Accepts: *string*

| Property | Accepts | Description |
| --- | --- | --- |
| `from=` (optional) | `asset` or `sidecar` or `manual` | Where the hash is refreshed from. `asset`, the default, hashes the payload itself. `sidecar` reads the `<url>.sha256` file that upstream publishes. `manual` means nothing recomputes the hash. |

> [!NOTE]
> If a pin has no `sha256` and no `unpinned`, then `tect check` reports an error.

<a id="imported-pin-path"></a>

#### `path` (optional)

The directory inside the archive that holds the content.

```kdl
imported "tectonic-os" {
    pin {
        path "modules/example"
    }
}
```

Accepts: *string*
