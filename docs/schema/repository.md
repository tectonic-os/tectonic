# The repository

A repository is a git repository with `repo.kdl` at its root, which is how `tect` finds the root. `tect create repo` scaffolds the layout below. Every path that `tect` reads or writes has a fixed place, so every repository looks alike.

```text
<repository>/
├── .github/
│   └── workflows/
├── generated/
├── keys/
│   ├── private/  (not in git)
│   └── public/
├── modules/
│   └── .remote/  (not in git)
├── out/  (not in git)
│   └── sources/  (not in git)
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
| `modules/` | the user, `tect create module` or `tect copy module` | yes | The modules of the repository, one directory each. See [Modules](modules.md). |
| `modules/.remote/` | `tect` | no | The modules that an image pins from another repository, fetched and verified against their hash. |
| `keys/public/` | `tect create key` or `tect set key` | yes | The public half of each key, at the path it has in the image. |
| `keys/private/` | `tect create key` | no | The private half of each key. The build never copies it into the image. |
| `generated/` | `tect generate` | yes | Everything that `tect generate` writes. `tect verify` compares it byte for byte, so the user does not edit it. |
| `scripts/` | `tect create scripts`, then the user | yes | The scripts that the repository keeps in place of the ones `tect generate` writes into `generated/scripts/`. |
| `.github/workflows/` | `tect generate` | yes | The CI workflows that `workflows` in `repo.kdl` names. |
| `out/` | `tect` | no | Local exports, caches and scratch. `tect` reads nothing here as a declaration. |
| `out/sources/` | `tect` | no | The collections that `tect` fetches. An imported module is read from here. |
| `.gitignore` | `tect create repo` | yes | Keeps `keys/private/`, `out/` and `modules/.remote/` out of git. |
