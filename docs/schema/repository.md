# The repository

A repository is a git repository with `repo.kdl` at its root, which is how `tect` finds the root. `tect create repo` scaffolds the layout below. Every path that `tect` reads or writes has a fixed place, so every repository looks alike.

```text
<repository>/
├── .cache/
│   └── sources/  (not in git)
├── .github/
│   ├── workflows/
│   └── renovate.json5
├── components/
│   └── components.lock.kdl
├── disk_config/
├── generated/
├── keys/
│   ├── private/  (not in git)
│   └── public/
├── out/  (not in git)
├── scripts/
├── .gitignore
├── <name>.image.kdl
├── image.kdl
└── repo.kdl
```

| Path | Written by | In git | Description |
| --- | --- | --- | --- |
| `repo.kdl` | `tect create repo`, then the user | yes | The settings of the whole repository. See [`repo.kdl`](repo.md). |
| `image.kdl` | `tect create image`, then the user | yes | The images of the repository. See [`image.kdl`](image.md). |
| `<name>.image.kdl` | `tect create image`, then the user | yes | More images. The name in front of the suffix is decorative. |
| `components/` | the user, `tect create component` or `tect import component` | yes | The components of the repository, one directory each. See [Components](components.md). |
| `components/components.lock.kdl` | `tect import component`, `tect fetch components` and `tect set component` | yes | Where each imported component came from, and what its content hashed to. See [`components.lock.kdl`](lock.md). |
| `keys/public/` | `tect create key` or `tect set key` | yes | The public half of each key, at the path it has in the image. |
| `keys/private/` | `tect create key` | no | The private half of each key. The build never copies it into the image. |
| `generated/` | `tect generate` | yes | Everything that `tect generate` writes. `tect verify` compares it byte for byte, so the user does not edit it. |
| `scripts/` | `tect create scripts`, then the user | yes | The scripts that the repository keeps in place of the ones `tect generate` writes into `generated/scripts/`. |
| `.github/workflows/` | `tect generate` | yes | The CI workflows that `workflows` in `repo.kdl` names. |
| `out/` | `tect` | no | Local exports, caches and scratch. `tect` reads nothing here as a declaration. |
| `.cache/sources/` | `tect` | no | The libraries that `tect import component` fetches and copies a component from. `tect check`, `tect generate` and `tect build` read nothing here. |
| `disk_config/` | `tect create repo`, then the user | yes | The settings that bootc-image-builder reads when it builds a Fedora or RHEL disk image. `disk.toml` sets the smallest size of `/`. |
| `.github/renovate.json5` | `tect generate` | yes | The Renovate settings, written from the `renovate` block in `repo.kdl`. Its custom managers keep the `tect` release, the base images, each pin that declares `renovate` and each image entry's tracked `version` current. |
| `.gitignore` | `tect create repo` | yes | Keeps `keys/private/`, `out/` and `.cache/` out of git. |
