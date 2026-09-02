# The installer live environment, and it is a layer on the image being
# installed rather than a distribution of its own. Tacklebox rebuilds the
# initramfs with this image's own dracut and boots the media from that kernel,
# so what it needs is the whole boot chain a bootc image has, and a plain
# `debian:forky` has none of it. Every payload has it by construction, which is
# why deriving is the cheap answer.
#
# Staged into out/bootiso/ by `tect vm build iso` and never committed: it
# varies with nothing but the recipe beside it, and eighty lines of installer
# plumbing in every repository buys nothing.
#
# Its cost is that the media carries the content roughly twice — this rootfs
# and the offline store are near-identical squashfs images and squashfs does
# not dedupe between them. 518 MB of ISO on an 800 MB payload, measured.

# renovate: datasource=docker depName=docker.io/library/golang
ARG GO_IMAGE=docker.io/library/golang:1.26
ARG PAYLOAD

# Neither project publishes a binary worth pinning — fisherman's one release
# asset is 174 commits behind its own tip, and tacklebox has no releases at all
# — so both are source archives pinned by sha256 and built here.
#
# The fisherman pin is `tuna-os`, deliberately, and its own repository
# description says otherwise: it reads `MOVED -> github.com/projectbluefin/
# fisherman`, and that destination is a fork whose tip is 123 commits and a
# month behind this one. The `composefs-backend requires fs-verity, which XFS
# does not support` guard — the one rule that catches a wrong `filesystem` in
# the recipe emitted beside this file — landed here ten days after the fork
# stopped. A redirect a project asserts about itself is not evidence of where
# its code is. Do not "correct" this pin.
#
# CGO_ENABLED=0 because tacklebox's default build links `net` and `os/user`
# against the builder's libc, and it is copied out of this stage to a host.
# Fisherman's Go module is at `fisherman/` inside its own repository and not at
# the root, which is why the two build directories below are not symmetrical.
FROM ${GO_IMAGE} AS tools
ARG FISHERMAN_COMMIT=027fa25c1d8bc01e2ac97d119cda9e8bb9c99ac7
ARG FISHERMAN_SHA256=ffab2a2c1094fa02a9b4862958c280045c9390425c93195855a9f0f93956c72e
ARG TACKLEBOX_COMMIT=b37b7a2fe9c47a551933e70d6ef023a00abda6ee
ARG TACKLEBOX_SHA256=760ddffbe234159f039508664c0112c663e0a550f41a6e2ffa4f6548bbd0baef
# `ExtractEFIBinary` takes an image argument, never reads it, and looks only at
# two host paths — so a host with no systemd-boot-unsigned is a hard stop, and
# on a cross-distro builder the host is the wrong source anyway: the media
# should boot with the bootloader its own image ships. The patch makes
# tacklebox's own error message true, and it is the one patch this project
# carries upstream. Report it rather than keep it.
COPY efi-from-image.patch /tmp/efi-from-image.patch
RUN set -eux; \
    fetch() { \
        curl --retry 3 -fsSLo "/tmp/$1.tar.gz" \
            "https://github.com/tuna-os/$1/archive/$2.tar.gz"; \
        echo "$3  /tmp/$1.tar.gz" | sha256sum -c -; \
        mkdir -p "/src/$1"; \
        tar -xf "/tmp/$1.tar.gz" -C "/src/$1" --strip-components=1; \
    }; \
    fetch fisherman "${FISHERMAN_COMMIT}" "${FISHERMAN_SHA256}"; \
    fetch tacklebox "${TACKLEBOX_COMMIT}" "${TACKLEBOX_SHA256}"; \
    git -C /src/tacklebox apply -p1 /tmp/efi-from-image.patch; \
    mkdir -p /out; \
    cd /src/fisherman/fisherman && CGO_ENABLED=0 go build -trimpath -o /out/fisherman ./cmd/fisherman/; \
    cd /src/tacklebox && CGO_ENABLED=0 go build -trimpath -o /out/tacklebox ./cmd/tacklebox

FROM ${PAYLOAD}

# podman runs the install container, so the family installing is irrelevant to
# the family installed and there is one live environment rather than one per
# family. fuse-overlayfs is what reads the offline store, and
# systemd-cryptenroll is what fisherman aborts before touching a disk without
# — Debian ships it in systemd-cryptsetup and not in systemd, and its own
# error text says to install systemd. tunaOS lost an image, an ISO and a live
# boot to that, so this asserts the binaries and never the packages.
#
# The fedora arm is not measured: no tect fedora image has assembled an ISO
# through this yet. The assertion below is what makes a wrong package name a
# failed ISO build rather than a wiped disk.
RUN set -eux; \
    if command -v apt-get > /dev/null 2>&1; then \
        apt-get update -y; \
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            podman fuse-overlayfs systemd-cryptsetup cryptsetup skopeo \
            fdisk dosfstools e2fsprogs xfsprogs; \
        apt-get clean -y; \
        rm -rf /var/lib/apt/lists/*; \
    else \
        dnf install -y --setopt=install_weak_deps=False \
            podman fuse-overlayfs cryptsetup skopeo \
            util-linux dosfstools e2fsprogs xfsprogs; \
        dnf clean all; \
    fi; \
    for tool in podman fuse-overlayfs skopeo cryptsetup systemd-cryptenroll \
        sfdisk mkfs.fat mkfs.ext4 mkfs.xfs; do \
        command -v "$tool" > /dev/null 2>&1 \
            || { echo "the live environment has no ${tool}" >&2; exit 1; }; \
    done

# Fisherman is the backend and nothing here reimplements partitioning, LUKS or
# TPM2 enrolment. Its recipe is baked in at a fixed path rather than written
# onto the media, because this layer is where the recipe is already known.
COPY --from=tools /out/fisherman /usr/bin/fisherman
COPY recipe.json /usr/share/tectonic/install-recipe.json

# Tacklebox writes the payload to LiveOS/store.squashfs.img and mounts the
# media at /run/initramfs/live, but ships no unit to mount the store — tunaOS
# carries its own. No path component here holds a dash, so the unit name is the
# mount point with slashes swapped and no \x2d escaping to get wrong.
COPY <<'MOUNT' /usr/lib/systemd/system/var-lib-tectonic-store.mount
[Unit]
Description=Offline image store carried by the installer media
ConditionPathExists=/run/initramfs/live/LiveOS/store.squashfs.img

[Mount]
What=/run/initramfs/live/LiveOS/store.squashfs.img
Where=/var/lib/tectonic/store
Type=squashfs
Options=ro,loop

[Install]
WantedBy=multi-user.target
MOUNT

# Naming the store in the recipe is not enough. `additionalImageStores` is
# handed to the bootc install container, while fisherman's pull step runs
# before that and is a plain `podman pull` that knows nothing about it. Until
# this file named the store too, every install tried the network.
COPY <<'CONF' /etc/containers/storage.conf
[storage]
driver = "overlay"
runroot = "/run/containers/storage"
graphroot = "/var/lib/containers/storage"

[storage.options]
additionalimagestores = ["/var/lib/tectonic/store"]

[storage.options.overlay]
mount_program = "/usr/bin/fuse-overlayfs"
CONF

# Root on the console, without a password, on installer media only. Fisherman
# partitions disks and calls `bootc install`, so a console that cannot become
# root cannot install anything — and this image is not the target: it is built
# per target as `<published>-installer`, boots only from the media, and is
# never what lands on the disk. When `tect install` autostarts here it will run
# as root for the same reason.
COPY <<'AUTOLOGIN' /usr/lib/systemd/system/serial-getty@.service.d/autologin.conf
[Service]
ExecStart=
ExecStart=-/sbin/agetty -o '-p -f -- \\u' --autologin root --keep-baud 115200,57600,38400,9600 - $TERM
AUTOLOGIN

COPY <<'AUTOLOGIN' /usr/lib/systemd/system/getty@.service.d/autologin.conf
[Service]
ExecStart=
ExecStart=-/sbin/agetty -o '-p -f -- \\u' --autologin root --noclear %I $TERM
AUTOLOGIN

RUN systemctl enable var-lib-tectonic-store.mount
