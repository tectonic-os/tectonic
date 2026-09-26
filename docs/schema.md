# The schema

Five kinds of file, all KDL, all read by `tect`.

| File | Declares |
| --- | --- |
| `repo.kdl` | the repository: the schema it is written against, the tool release it pins, which image a bare build builds, which CI it generates |
| `image.kdl` or `<name>.image.kdl` at the root | one image file, holding what each image calls itself, what it builds on, and the modules in it |
| `modules/<path>/module.kdl` | one module: what it needs, what it offers, and what an image author may set |
| `bases.kdl`, at a collection's root | the bases a collection describes, which extend the ones the tool ships with |
| `provenance.kdl`, beside a copied module | where the module was copied from, and what its content hashed to then |

`tect check` reads them and reports every problem at the line that caused it.
Each schema is documented in the file for its reader area, listed below.
The reference tables in those files are generated from the tables the parser
walks, so they cannot drift from what the tool accepts.

<!-- schema: index -->

| Schema | Documented in | Meaning |
| --- | --- | --- |
| `repo` | [`schema/repo.md`](schema/repo.md) | What is true of the repository, leaving each image to say what is true of itself. |
| `image` | [`schema/image.md`](schema/image.md#image) | One image: what it calls itself, what it builds on, and everything it is made of. |
| `bases` | [`schema/bases.md`](schema/bases.md) | The bases a collection describes, which extend the ones the tool ships with. |
| `module` | [`schema/module.md`](schema/module.md) | One module: what it builds on, what it needs from the rest, and what it installs. |
| `option` | [`schema/module.md`](schema/module.md#option) | One value an image may set on this module, reaching the build as OPT_*. |
| `variant` | [`schema/module.md`](schema/module.md#variant) | A named set of option values an image selects with `variant=`. |
| `asset` | [`schema/pins.md`](schema/pins.md#asset) | A pinned upstream payload the module fetches, reaching the build as ASSET_*. |
| `pin` | [`schema/pins.md`](schema/pins.md#pin) | Where this comes from, which version of it, what proves you got that one, and what keeps the selector current. |
| `imported` | [`schema/pins.md`](schema/pins.md#imported) | Where this module was copied from, and what its content hashed to then. Written by `tect copy module`; the module's author does not maintain it. |

<!-- /schema: index -->
