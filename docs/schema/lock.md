<a id="components.lock.kdl"></a>

# `components.lock.kdl`

Location: `components/components.lock.kdl`, at the root of `components/`.

When `tect import component` copies a component into this repository, it records the component here. The entry keeps where the component came from and what its content hashed to, so `tect check`, `tect generate` and `tect build` can tell whether the copy has changed since.

```kdl
component "browser" {
    library "tectonic-os"
    path "browser"
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

| Field | Accepts | Description |
| --- | --- | --- |
| [`component`](#component) (required,&nbsp;unique) | *string*, then {&nbsp;[fields](#component-fields)&nbsp;} | One imported component, by its path under `components/`. |

> [!NOTE]
> `tect import component` writes the whole file. The user does not edit it, and every command that reads the images refuses an entry that does not read.
>
> `tect fetch components` copies each tracked component again from its library, and records the new `content` and `component-version`. `tect set component <path> --untracked` drops `track` and keeps the rest of the entry.
>
> The entry is keyed by the local path, so one library component imported twice under two names has two entries.
>
> `plan.json` carries the same hash for each component, so `tect verify` fails on a component edited without a new `tect generate`.
>
> If a tracked component's content no longer matches its entry, or its directory is missing, then every command that reads the images refuses it, `tect build` included. `audit` does not change this.
>
> If an untracked component's content no longer matches its entry, then `tect why` reads the edit out. The copy is the repository's own, so no command refuses it.

<a id="component"></a>

## `component` (required, unique)

One imported component, by its path under `components/`.

```kdl
component "browser" {
    library "tectonic-os"
    path "browser"
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

Accepts: *string*, then {&nbsp;[fields](#component-fields)&nbsp;}

<a id="component-fields"></a>

| Field | Accepts | Description |
| --- | --- | --- |
| [`library`](#component-library) (required) | *string* | The library the component was imported from, as `sources` in `repo.kdl` names it. |
| [`path`](#component-path) (required) | *string* | The path of the component inside the library. It differs from the entry's own path when the component was imported under another name. |
| [`url`](#component-url) (optional) | *string* | The Git URL of the library, for a library that `tect` fetched. |
| [`component-version`](#component-component-version) (optional) | *string* | The revision of the library that the component was taken at, for a library that `tect` fetched. |
| [`content`](#component-content) (required) | *string* | The hash that the component directory had at import. |
| [`track`](#component-track) (optional) |  | Marks a copy that tracks its library. An untracked copy is the repository's own. |

<a id="component-library"></a>

### `library` (required)

The library the component was imported from, as `sources` in `repo.kdl` names it.

```kdl
component "browser" {
    library "tectonic-os"
}
```

Accepts: *string*

<a id="component-path"></a>

### `path` (required)

The path of the component inside the library. It differs from the entry's own path when the component was imported under another name.

```kdl
component "browser" {
    path "browser"
}
```

Accepts: *string*

<a id="component-url"></a>

### `url` (optional)

The Git URL of the library, for a library that `tect` fetched.

```kdl
component "browser" {
    url "https://github.com/tectonic-os/library.git"
}
```

Accepts: *string*

<a id="component-component-version"></a>

### `component-version` (optional)

The revision of the library that the component was taken at, for a library that `tect` fetched.

```kdl
component "browser" {
    component-version "v0.6.51"
}
```

Accepts: *string*

<a id="component-content"></a>

### `content` (required)

The hash that the component directory had at import.

```kdl
component "browser" {
    content "b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3"
}
```

Accepts: *string*

<a id="component-track"></a>

### `track` (optional)

Marks a copy that tracks its library. An untracked copy is the repository's own.

```kdl
component "browser" {
    track
}
```
