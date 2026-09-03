# The installer live environment. It is a fixed Fedora base rather than a layer
# on the image being installed, and that is a decision about Secure Boot.
#
# The media's boot chain decides whether a stranger's machine will boot the
# stick at all, and it is independent of what gets installed: fisherman runs
# the target image through podman, so the family installing is not the family
# installed. Fedora ships a shim signed by the Microsoft UEFI CA that firmware
# already trusts; Debian ships an unsigned systemd-boot and no shim, which
# firmware with Secure Boot on — the factory default — refuses with `Access
# Denied` before the installer draws anything. Deriving from the payload made
# that refusal a property of which target was being installed, which is the
# wrong thing for it to depend on. Fedora/RHEL-derived images are also most of
# the bootc ecosystem, so this is the best-travelled live environment rather
# than merely the one that boots signed.
#
# Measured 2026-09-03: media built this way boots from a USB block device under
# Secure Boot firmware with the default keys enrolled, and the kernel reports
# `Kernel is locked down from EFI Secure Boot mode` — the whole chain verified,
# with no key for anyone to enrol.
#
# Two costs, both real and neither hidden. The media no longer carries the
# payload's own kernel, so a media that boots no longer proves the installed
# system will. And a stick for a non-Fedora target grows — measured 996 MB to
# 1.88 GB for the Debian one — because the live rootfs and the offline store
# are then different images with nothing to share.
#
# What this does NOT fix: a Debian target still installs an unsigned
# systemd-boot onto the disk, so the installed machine still needs Secure Boot
# off. That is the family table's to answer, not this file's.
#
# Staged into out/bootiso/ by `tect vm build iso` and never committed: it
# varies with nothing but the recipe beside it, and eighty lines of installer
# plumbing in every repository buys nothing.

# renovate: datasource=docker depName=docker.io/library/golang
ARG GO_IMAGE=docker.io/library/golang:1.26
# renovate: datasource=docker depName=quay.io/fedora/fedora-bootc
ARG LIVE_BASE=quay.io/fedora/fedora-bootc:44

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
ARG FISHERMAN_ORG=tuna-os
ARG FISHERMAN_COMMIT=027fa25c1d8bc01e2ac97d119cda9e8bb9c99ac7
ARG FISHERMAN_SHA256=ffab2a2c1094fa02a9b4862958c280045c9390425c93195855a9f0f93956c72e
# Tacklebox is this project's own fork, and unlike the fisherman pin above that
# is not a preference about where the code lives — the media needs two changes
# upstream has not got. The pin is the fork's `tectonic` branch, which is those
# two and nothing else: each is also a branch of its own — `secure-boot-media`
# and `esp-partition` — scoped for submission upstream separately, because they
# are independent and only one of them is about Secure Boot. It stages the payload's signed shim so the stick boots a
# machine with Secure Boot on, which is the factory default and refused the
# unsigned systemd-boot with `Access Denied`; and it appends the ESP as a
# partition so firmware finds it on a USB block device rather than only through
# an El Torito scan. Report both upstream rather than keep them.
ARG TACKLEBOX_ORG=tectonic-os
ARG TACKLEBOX_COMMIT=06d62ed0452958cec17a5f95184a7a3ce5f34032
ARG TACKLEBOX_SHA256=09edec7c20c9900871c4243247852656afcb20e3ab42cbe9bd9f6374aa78f07b
# `ExtractEFIBinary` takes an image argument, never reads it, and looks only at
# two host paths — so a host with no systemd-boot-unsigned is a hard stop, and
# on a cross-distro builder the host is the wrong source anyway: the media
# should boot with the bootloader its own image ships. The patch makes
# tacklebox's own error message true, and it is the one patch this project
# carries upstream. Report it rather than keep it.
COPY efi-from-image.patch /tmp/efi-from-image.patch
RUN set -eux; \
    fetch() { \
        curl --retry 3 -fsSLo "/tmp/$2.tar.gz" \
            "https://github.com/$1/$2/archive/$3.tar.gz"; \
        echo "$4  /tmp/$2.tar.gz" | sha256sum -c -; \
        mkdir -p "/src/$2"; \
        tar -xf "/tmp/$2.tar.gz" -C "/src/$2" --strip-components=1; \
    }; \
    fetch "${FISHERMAN_ORG}" fisherman "${FISHERMAN_COMMIT}" "${FISHERMAN_SHA256}"; \
    fetch "${TACKLEBOX_ORG}" tacklebox "${TACKLEBOX_COMMIT}" "${TACKLEBOX_SHA256}"; \
    git -C /src/tacklebox apply -p1 /tmp/efi-from-image.patch; \
    mkdir -p /out; \
    cd /src/fisherman/fisherman && CGO_ENABLED=0 go build -trimpath -o /out/fisherman ./cmd/fisherman/; \
    cd /src/tacklebox && CGO_ENABLED=0 go build -trimpath -o /out/tacklebox ./cmd/tacklebox

FROM ${LIVE_BASE}

# podman runs the install container, so the family installing is irrelevant to
# the family installed and there is one live environment rather than one per
# family. fuse-overlayfs is what reads the offline store, and
# systemd-cryptenroll is what fisherman aborts before touching a disk without
# — Debian ships it in systemd-cryptsetup and not in systemd, and its own
# error text says to install systemd. tunaOS lost an image, an ISO and a live
# boot to that, so this asserts the binaries and never the packages.
#
# openssl is `tect install`'s password hash. Fisherman hands the recipe's
# password to chpasswd, and only a `$`-prefixed crypt string takes the `-e`
# branch: a plaintext one goes through PAM and dies after the OS is already on
# the disk. `openssl passwd -6 -stdin` is what produces it, and crypt(3) is not
# an option here — glibc keeps it in libcrypt rather than libc.
#
# The dnf arm is the one that runs, since the base above is Fedora. The apt arm
# is kept because `LIVE_BASE` is overridable and a Debian live environment is
# the fallback for anyone who cannot use this one — it produces unsigned media.
# Measured 2026-09-03: the dnf arm assembles an ISO that boots. The assertion
# below is what makes a wrong package name a failed ISO build rather than a
# wiped disk.
RUN set -eux; \
    if command -v apt-get > /dev/null 2>&1; then \
        apt-get update -y; \
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            podman fuse-overlayfs systemd-cryptsetup cryptsetup skopeo openssl \
            fdisk dosfstools e2fsprogs xfsprogs; \
        apt-get clean -y; \
        rm -rf /var/lib/apt/lists/*; \
    else \
        dnf install -y --setopt=install_weak_deps=False \
            podman fuse-overlayfs cryptsetup skopeo openssl \
            util-linux dosfstools e2fsprogs xfsprogs; \
        dnf clean all; \
    fi; \
    for tool in podman fuse-overlayfs skopeo cryptsetup systemd-cryptenroll \
        openssl sfdisk mkfs.fat mkfs.ext4 mkfs.xfs; do \
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
# never what lands on the disk. The `tect install` that autostarts below runs
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

# The frontend, staged into this build context from the running binary by
# `tect vm build iso`. `--version` runs it here rather than on the console, so
# a binary that cannot execute in this environment fails the ISO build instead
# of the boot it was assembled for.
COPY tect /usr/bin/tect
RUN /usr/bin/tect --version

# Autostart is a login shell's profile and not a unit, and that is the whole
# reason it is one line: root already autologins on both consoles above, an
# installer that leaves — esc on the first screen — falls back to the shell it
# was started from rather than to a dead service, and there is no tty to hand
# between a unit and a getty. `/etc/profile.d` is read by bash and sh alike on
# both families. `ui::inline` sets the window size itself, so a serial console
# reporting none needs nothing here.
COPY <<'START' /etc/profile.d/tect-install.sh
if [ "$(id -u)" = 0 ] && [ -t 0 ]; then
    tect install
fi
START
