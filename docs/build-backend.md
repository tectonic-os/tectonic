# Build backends

`tect build` uses one of two container build tools. The choice changes where
the image lands, whether the build caches layers in a registry, and whether a
UKI image can be built.

## Which backend to use

`buildah` is the default. Use it for a UKI image, for a local VM build, or when
you do not need a shared layer cache.

`buildx` caches layers in a registry. Use it when you want that cache shared
between runs and flavours. `buildx` cannot build a UKI image.

## Choose a backend

Set `backend "buildx"` in `repo.kdl` to choose `buildx` for the repository.
`--backend <name>` overrides the choice for one command. `$BUILD_BACKEND`
overrides it while the variable is set. The flag wins over the variable. Either
wins over `repo.kdl`.

Run `tect generate` after you change the backend. The generated `build.yml`
follows the declared backend.

## What changes

| | `buildah` | `buildx` |
| --- | --- | --- |
| Image destination | The local podman store. | The Docker store, or an OCI archive with `--oci-output`. |
| Base image | A newer base is pulled on every build. | The builder's default pull policy applies. |
| Registry layer cache | None. `--cache-to` is refused. | The cache described below. |
| UKI image | Built. | Refused. |

`buildx` also turns off the provenance attestation that BuildKit would add.

## Registry cache

`buildx` reads and writes a layer cache in `<namespace>/<default-image>-cache`.
The namespace is `$IMAGE_REGISTRY`, or the GitHub origin when that variable is
unset. The default image is the only image, or the image named by
`default-image`.

By default, `buildx` reads the cache for the target and the other flavours of
the same image. `--no-cache-from` skips that read. `--cache-to` writes the
cache.

A repository with more than one image must declare `default-image`. Without a
name for the cache, `buildx` skips it. `--cache-to` then fails.

## UKI images

An image with `boot-chain "uki-shim"` or `boot-chain "uki-db"` builds a UKI.
Only `buildah` can build it. `tect check` refuses `backend "buildx"` for such a
repository. `tect build --backend buildx` refuses that target.

A UKI build runs in two passes. The first builds the image. The second seals it
with a digest read from the local podman store. `buildx` does not use that
store. `tect` tags the image only after the sealed image passes its checks. A
failed check publishes nothing.

## In CI

The generated `build.yml` runs the declared backend.

- `buildah`: the image lands in the root podman store. The SBOM, rechunk, push
  and signing steps read that store.
- `buildx`: `tect build` writes an OCI archive. The workflow copies it into the
  root podman store. The later steps do not change. A publishing run also
  writes the layer cache.

## In a local VM

`tect vm run --rebuild` always uses `buildah`, whatever `repo.kdl` declares.
The VM boots the image from the podman store. A local VM build of a `buildx`
repository is therefore a `buildah` build.

## What the build produces

`buildah` leaves the image in the local podman store. `buildx` loads it into
the Docker store, or writes an OCI archive with `--oci-output`.

Each image carries two files. `/usr/share/tectonic/build.json` is the resolved
build record, and it names the backend. `/usr/share/tectonic/manifest.json` is
the generated plan. If `repo.kdl` sets `manifest { label #true }`, the image
also carries an `org.tectonic.manifest` label that points at the plan.
