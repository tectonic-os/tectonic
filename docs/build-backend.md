# Build backends

`tect build` hands one generated Containerfile to a container build tool. That
tool is the backend. This page lists everything the backend receives and
everything it returns, so a reader can tell what a second backend would have
to honour.

`tect` drives two backends:

| Backend | Program | Default |
| --- | --- | --- |
| `buildah` | `podman build` | yes |
| `buildx` | `docker buildx build` | no |

The repository chooses one with [`backend`](schema/repo.md#backend) in
`repo.kdl`. `--backend` or `$BUILD_BACKEND` overrides it for one build. The
flag wins over the variable, and the variable wins over `repo.kdl`.

## Inputs

Every build receives these inputs, whichever backend runs it.

| Input | Form | Source |
| --- | --- | --- |
| Containerfile | `--file generated/<image>/Containerfile` | `tect generate` writes it, and `tect build` refuses a stale one |
| Context | the repository root | the directory `tect build` runs in |
| Build arguments | `--build-arg NAME=value` | the table below |
| Secrets | `--secret id=<id>,src=<path>` | each `--secret <id>=<path>` given to `tect build`, and `mok_privkey` from `$MOK_KEY_PATH` |
| Tags | `--tag <reference>` | each `--tag` and each line of `$TAGS`; with neither, `$IMAGE_NAME:$DEFAULT_TAG`, whose parts default to the published name and `latest` |
| Labels | `--label <key>=<value>` | `$LABELS`, and the manifest label if `manifest { label #true }` |

The build arguments carry what the plan resolved, so the build itself resolves
nothing:

| Argument | What it holds |
| --- | --- |
| `FLAVOUR` | The flavour of the target, empty for the image itself. |
| `IMAGE_VERSION` | `$IMAGE_VERSION`, else today's date in UTC. |
| `IMAGE_REGISTRY` | The registry namespace the image publishes under. |
| `BASE`, `BASE_DECLARED` | The base as resolved to a digest, and as declared. |
| `KERNEL` | The `--kernel` value, if given. |
| `TARGET` | The target, as `<image>` or `<image>/<flavour>`. |
| `FAMILY` | The base family. |
| `CONFORMS`, `SCAP_CONTENT`, `SCAP_CLAIMED`, `SCAP_REFUSED` | The declared benchmark profile, its content, and the rules the components claim or refuse. |
| `CONTRACT_FILES`, `VERIFY_EXCEPTIONS`, `COMPONENT_PRESETS`, `LUKS_INITRAMFS` | What `tect validate-image` checks inside the finished image. |
| `COMPONENT_HASHES`, `ASSET_RESOLUTIONS`, `SOURCE_COMMIT`, `BUILD_BACKEND`, `AUDIT_ENFORCE` | What the build record states. |

The image file's own values reach the Containerfile as `ARG` defaults, because
they change only with the image file: `IMAGE_ID`, `IMAGE_NAME`,
`IMAGE_PRETTY_NAME`, `IMAGE_URL` and `IMAGE_ISSUES_URL`. The finalize layer
passes them to every component's finalize hook.

## What each backend adds

| Input | `buildah` | `buildx` |
| --- | --- | --- |
| Pull policy | `--pull=newer` | the builder's default |
| Registry layer cache | the import is ignored with a note, and `--cache-to` is refused | `--cache-from` for the target and its sibling flavours, and `--cache-to` with `--cache-to`. The cache repository is `<default image>-cache`, so `tect check` requires `default-image` under `buildx` |
| OCI archive | refused | `--output type=oci,dest=<path>` with `--oci-output <path>`, else `--load` into the Docker store |
| Provenance attestation | none | `--provenance=false` |

## Outputs

| Output | Where |
| --- | --- |
| The image | The backend's own store under each tag. With `--oci-output`, an OCI archive instead. |
| The build record | `/usr/share/tectonic/build.json` inside the image. |
| The declared manifest | `/usr/share/tectonic/manifest.json` inside the image, and an OCI label if declared. |

A single-pass build replaces the `tect` process with the backend, so the
backend's exit status is the build's exit status.

## The composefs digest handoff

An image whose `boot-chain` builds a UKI is built in two passes, and only
`buildah` can build it. `tect check` refuses `backend "buildx"` for such an
image, and `tect build` refuses `--backend buildx` for its target.

1. The first pass builds the `split` stage under a scratch tag.
2. `tect` runs `bootc container compute-composefs-digest-from-storage` on the
   split image in a container that mounts the podman store. This is the digest
   `bootc install` compares against.
3. The second pass builds the sealed image from the generated file's tail. It
   receives `SPLIT_BASE`, the split image's tag, and `COMPOSEFS_DIGEST`, the
   digest. The UKI embeds the digest on its kernel command line.
4. `tect` computes the sealed image's own digest and refuses it if the digest
   moved. It reads the UKI command line back, and it runs `tect
   validate-image` against the sealed image.
5. Only then does `tect` apply the requested tags. A failed check publishes
   nothing and removes both scratch images.

The handoff needs the split image in a store that a container can mount, and a
BuildKit builder keeps its images in its own container. That is why `buildx`
cannot build a UKI.

## In CI

The generated `build.yml` follows `backend`:

- **`buildah`:** `tect build` runs as root, so the image lands in the root
  podman store that the SBOM, rechunk, push and signing steps read.
- **`buildx`:** the job sets up a BuildKit builder. `tect build` writes an OCI
  archive, and `skopeo` copies it into the root podman store under the same
  staging tag. On a publishing run, `tect build` also exports the layer cache.
  The later steps run unchanged.

## On the user's machine

`tect vm run --rebuild` builds with `buildah` whatever `repo.kdl` declares,
because the VM boots the image from the podman store.
