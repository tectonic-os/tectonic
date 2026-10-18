<a id="modules.lock.kdl"></a>

# `modules.lock.kdl`

Location: `modules/modules.lock.kdl`, at the root of `modules/`.

When `tect import module` copies a module into this repository, it records the module here. The entry keeps where the module came from and what its content hashed to, so `tect check` can show whether the copy has changed since.

```kdl
module "browser" {
    library "tectonic-os"
    path "browser"
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`module`](#module) (required,&nbsp;unique) | *string*, then {&nbsp;[fields](#module-fields)&nbsp;} | One imported module, by its path under `modules/`. |

> [!NOTE]
> `tect import module` writes the whole file. The user does not edit it, and `tect check` reports an entry that does not read.
>
> The entry is keyed by the local path, so one library module imported twice under two names has two entries.
>
> `plan.json` carries the same hash for each module, so `tect verify` fails on a module edited without a new `tect generate`.
>
> If a module's content no longer matches its entry, then `tect check` names the module. The mismatch is an error only under `audit { enforce #true }`.

<a id="module"></a>

## `module` (required, unique)

One imported module, by its path under `modules/`.

```kdl
module "browser" {
    library "tectonic-os"
    path "browser"
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

Accepts: *string*, then {&nbsp;[fields](#module-fields)&nbsp;}

<a id="module-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`library`](#module-library) (required) | *string* | The library the module was imported from, as `sources` in `repo.kdl` names it. |
| [`path`](#module-path) (required) | *string* | The path of the module inside the library. It differs from the entry's own path when the module was imported under another name. |
| [`url`](#module-url) (optional) | *string* | The Git URL of the library, for a library that `tect` fetched. |
| [`component-version`](#module-component-version) (optional) | *string* | The revision of the library that the module was taken at, for a library that `tect` fetched. |
| [`content`](#module-content) (required) | *string* | The hash that the module directory had at import. |
| [`track`](#module-track) (optional) |  | Marks a copy that tracks its library. An untracked copy is the repository's own. |

<a id="module-library"></a>

### `library` (required)

The library the module was imported from, as `sources` in `repo.kdl` names it.

```kdl
module "browser" {
    library "tectonic-os"
}
```

Accepts: *string*

<a id="module-path"></a>

### `path` (required)

The path of the module inside the library. It differs from the entry's own path when the module was imported under another name.

```kdl
module "browser" {
    path "browser"
}
```

Accepts: *string*

<a id="module-url"></a>

### `url` (optional)

The Git URL of the library, for a library that `tect` fetched.

```kdl
module "browser" {
    url "https://github.com/tectonic-os/library.git"
}
```

Accepts: *string*

<a id="module-component-version"></a>

### `component-version` (optional)

The revision of the library that the module was taken at, for a library that `tect` fetched.

```kdl
module "browser" {
    component-version "v0.6.51"
}
```

Accepts: *string*

<a id="module-content"></a>

### `content` (required)

The hash that the module directory had at import.

```kdl
module "browser" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

Accepts: *string*

<a id="module-track"></a>

### `track` (optional)

Marks a copy that tracks its library. An untracked copy is the repository's own.

```kdl
module "browser" {
    track
}
```
