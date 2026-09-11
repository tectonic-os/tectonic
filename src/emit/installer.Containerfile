# The installer live environment. `tect vm build iso` stages this into
# out/bootiso/ and sets `LIVE_BASE` to the payload itself for a dnf family; the
# Fedora default serves the rest, whose images tacklebox stages unsigned.

# renovate: datasource=docker depName=docker.io/library/golang
ARG GO_IMAGE=docker.io/library/golang:1.26
# renovate: datasource=docker depName=quay.io/fedora/fedora-bootc
ARG LIVE_BASE=quay.io/fedora/fedora-bootc:44

# Neither project publishes a binary worth pinning, so both are source archives
# pinned by sha256 and built here.
#
# Fisherman is pinned to upstream and this project carries no fisherman fork.
# The base it forks from is `tuna-os` deliberately: that repository's own
# description reads `MOVED -> github.com/projectbluefin/fisherman`, and that
# destination is a fork well behind this one. Do not "correct" this pin.
#
# CGO_ENABLED=0 because tacklebox's default build links `net` and `os/user`
# against the builder's libc, and it is copied out of this stage to a host.
# Fisherman's Go module is at `fisherman/` inside its own repository and not at
# the root, which is why the two build directories below are not symmetrical.
FROM ${GO_IMAGE} AS tools
ARG FISHERMAN_ORG=tuna-os
ARG FISHERMAN_COMMIT=027fa25c1d8bc01e2ac97d119cda9e8bb9c99ac7
ARG FISHERMAN_SHA256=ffab2a2c1094fa02a9b4862958c280045c9390425c93195855a9f0f93956c72e
# Tacklebox is the one fork left: the media needs a change upstream has not got.
# The pin is `secure-boot-media`, which stages the payload's signed shim so the
# stick boots a machine with Secure Boot on. Upstream has most of this in
# `purefs.DetectBootChain`, wired only into `cmd/purebuild` and `cmd/tbwasm`.
#
# The pin also carries a fourth layout for the signed pair: the deb families
# keep theirs under `/usr/lib/shim` and `/usr/lib/grub/x86_64-efi-signed`, which
# none of the three upstream layouts covers, so a deb `LIVE_BASE` fell through
# to unsigned media without saying so. Inert while `LIVE_BASE` is Fedora.
ARG TACKLEBOX_ORG=tectonic-os
ARG TACKLEBOX_COMMIT=b3f3b9a744d65c93dc2536cc55e4bb3030e0535c
ARG TACKLEBOX_SHA256=3c9d9904d6ae0ef9fbd949435c63d1c20ab0cab940543b17668fd94e0330e278
# `ExtractEFIBinary` takes an image argument, never reads it, and looks only at
# two host paths, so a host with no systemd-boot-unsigned is a hard stop and on
# a cross-distro builder the host is the wrong source. The patch makes
# tacklebox's own error message true.
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
# the family installed and there is one live environment. fuse-overlayfs reads
# the offline store; systemd-cryptenroll is what fisherman aborts before
# touching a disk without, and Debian ships it in systemd-cryptsetup and not in
# systemd. So this asserts the binaries and never the packages.
#
# openssl is the installer's password hash: fisherman hands the recipe's
# password to chpasswd, and only a `$`-prefixed crypt string takes the `-e`
# branch. crypt(3) is not an option — glibc keeps it in libcrypt.
#
# The apt arm produces unsigned media. EL packages no kmscon, so it is asked
# for alone. The assertion below makes a wrong package name a failed ISO build
# and not a wiped disk.
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
        dnf install -y --setopt=install_weak_deps=False kmscon || true; \
        dnf clean all; \
    fi; \
    for tool in podman fuse-overlayfs skopeo cryptsetup systemd-cryptenroll \
        openssl sfdisk mkfs.fat mkfs.ext4 mkfs.xfs; do \
        command -v "$tool" > /dev/null 2>&1 \
            || { echo "the live environment has no ${tool}" >&2; exit 1; }; \
    done; \
    command -v kmscon > /dev/null 2>&1 \
        || echo "no kmscon here; the console falls back to the kernel VT" >&2

# The media is not the machine it installs, so a hardened payload's rules that
# stop a stick booting or its console being driven are lifted here: modules the
# media boots through, services that block USB, binaries or a full audit log,
# and the shell timeout. The initramfs tacklebox builds reads this modprobe.d.
ARG MEDIA_MODULES="usb_storage uas squashfs loop overlay isofs vfat"
RUN set -eux; \
    pattern="$(echo "$MEDIA_MODULES" | sed 's/_/[-_]/g; s/ /|/g')"; \
    find /etc/modprobe.d /usr/lib/modprobe.d -name '*.conf' -type f -exec sed -i -E \
        "/^[[:space:]]*(install|blacklist)[[:space:]]+(${pattern})([[:space:]]|\$)/d" {} +; \
    kver="$(find /usr/lib/modules -mindepth 1 -maxdepth 1 -printf '%f\n' | head -n1)"; \
    blocked="$(modprobe -S "$kver" -c 2> /dev/null \
        | grep -E "^(install|blacklist) ($(echo "$MEDIA_MODULES" | tr ' ' '|'))( |\$)" || true)"; \
    [ -z "$blocked" ] || { echo "the media cannot load: ${blocked}" >&2; exit 1; }; \
    systemctl mask usbguard.service fapolicyd.service auditd.service; \
    grep -rlZ TMOUT /etc/profile /etc/profile.d /etc/bashrc /etc/bash.bashrc 2> /dev/null \
        | xargs -0r sed -i '/TMOUT/d'; \
    systemctl set-default multi-user.target

# Fisherman is the backend and nothing here reimplements partitioning, LUKS or
# TPM2 enrolment. Its recipe is baked in at a fixed path rather than written
# onto the media, because this layer is where the recipe is already known.
COPY --from=tools /out/fisherman /usr/bin/fisherman
COPY recipe.json /usr/share/tectonic/install-recipe.json

# Tacklebox writes the payload to LiveOS/store.squashfs.img and mounts the media
# at /run/initramfs/live, but ships no unit to mount the store. No path
# component here holds a dash, so the unit name is the mount point with slashes
# swapped and no \x2d escaping to get wrong.
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
# handed to the bootc install container, while fisherman's pull step runs before
# that and is a plain `podman pull` that knows nothing about it.
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
# root cannot install anything. This image is built per target as
# `<published>-installer`, boots only from the media, and never lands on a disk.
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

# Kernel messages go to the journal and not to the console. The installer draws
# a bounded box and redraws only the cells it changed, so a `printk` landing in
# the middle of it stays there until something else writes that cell. `4` is the
# default for everything but the console level, which drops to `1`: a panic
# still reaches the screen, and `journalctl` still has all of it.
COPY <<'QUIET' /usr/lib/sysctl.d/50-tect-installer-console.conf
kernel.printk = 1 4 1 4
QUIET

# The kernel's own console draws a bitmap font of at most 512 glyphs in sixteen
# colours, and no console font carries the box-drawing arcs this screen uses.
# `setfont` loads bitmaps, so a TTF is not an answer either. kmscon draws on DRM
# through pango, and `monospace` already resolves to Adwaita Mono in this base.
#
# The serial console is untouched: it is how this media is driven headless, and
# a graphical console that fails must not take the headless path with it.
COPY <<'KMSCON' /usr/lib/systemd/system/kmsconvt@.service.d/autologin.conf
[Service]
ExecStart=
ExecStart=kmscon --vt=%I --no-switchvt --login -- /bin/login -f root
KMSCON

# `kmsconvt@.service` ships `OnFailure=getty@%i.service`, so a kmscon that
# cannot open DRM hands tty1 back to the plain VT with its own autologin above.
# The enable is guarded because the apt arm of this file has no such package.
RUN systemctl enable var-lib-tectonic-store.mount \
    && { [ ! -f /usr/lib/systemd/system/kmsconvt@.service ] \
        || systemctl enable kmsconvt@tty1.service; }

# The frontend, staged into this build context from the running binary by
# `tect vm build iso`. `--version` runs it here, so a binary that cannot execute
# in this environment fails the ISO build instead of the boot.
COPY tect /usr/bin/tect
RUN /usr/bin/tect --version

# Autostart is a login shell's profile and not a unit: root already autologins
# on both consoles above, an installer answering `Leave to a shell` falls back
# to the shell it was started from, and there is no tty to hand between a unit
# and a getty. `/etc/profile.d` is read by bash and sh alike on both families.
# `ui::inline` sets the window size itself.
#
# The word here is the command table's. Nothing else ties a command typed as
# text to the table that resolves it, so a rename leaves the media booting to
# `unknown command`; the test
# `the_verb_the_live_environment_autostarts_is_one_that_resolves` is the tie.
COPY <<'START' /etc/profile.d/tect-installer.sh
if [ "$(id -u)" = 0 ] && [ -t 0 ]; then
    tect installer
fi
START
