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
# Fisherman is pinned to a fork again since 2026-09-12, for `varDisk.size`,
# which cuts /var out of the install disk, and since 2026-09-16 for the retag
# that lets an encrypted root boot under a sealed UKI, the composefs user
# creation fix (the deploy root's /var is a stateroot symlink, and login.defs
# makes useradd create the home by default) and the first-boot TPM2 unit going
# into the deployment's /etc instead of the physical root's, which no boot
# reads. The branch above those carries the `tpm2-luks-pin` encryption kind,
# drops the payload's signatures from the OCI export so a signed source can
# install, and holds every login until the first-boot enrollment finishes,
# with a 120-second start cap so a hung TPM cannot lock the machine out. The
# PCR-policy change is stacked
# on `tectonic-integration`, which holds the other changes. The fork is
# of `tuna-os` deliberately: that repository's own
# description reads `MOVED -> github.com/projectbluefin/fisherman`, and that
# destination is a fork well behind this one. Do not "correct" the base it
# forks from.
#
# CGO_ENABLED=0 because tacklebox's default build links `net` and `os/user`
# against the builder's libc, and it is copied out of this stage to a host.
# Fisherman's Go module is at `fisherman/` inside its own repository and not at
# the root, which is why the two build directories below are not symmetrical.
FROM ${GO_IMAGE} AS tools
ARG FISHERMAN_ORG=tectonic-os
ARG FISHERMAN_COMMIT=c7033b17fd7ccc5f71befe4f77bf93c7ebadb831
ARG FISHERMAN_SHA256=1f826c102672c101e02b38448c22a4894cab12c4822983b16e1418fd8bfa5462
# Tacklebox is the other fork: the media needs a change upstream has not got.
# The pin is `feat/grub-bootloader-support`, which stages the live image's own
# bootloader, a signed shim and GRUB pair in any of four layouts, the deb
# families' `/usr/lib/shim` included. It keeps upstream's systemd-boot first,
# and `vm.sh` asks for the pair with `--media-bootloader grub2`.
ARG TACKLEBOX_ORG=tectonic-os
ARG TACKLEBOX_COMMIT=da820e5e9ac34c7a851382a9a1c7a4c6bd0c886f
ARG TACKLEBOX_SHA256=c873e6a102640b2688de126238e2faae3abfd1c40370bf46df2906965d9c0439
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

# The installer does publish a binary worth pinning, so this one is fetched and
# not built. It is staged here rather than in the live environment below
# because this stage already has curl and tar, and the live base is asserted
# for install tools and not for build tools.
#
# `uname -m` and not a build argument: this stage runs on the architecture the
# media is built for, and the release names its assets with the same two words.
# The tarball holds the binary at the top level with nothing beside it, so no
# `--strip-components` is passed. Against a single top-level file that flag
# extracts nothing and still exits 0.
#
# These three are bumped by hand, as the fisherman and tacklebox pins above
# are. A `# renovate:` directive here would do nothing: the dockerfile manager
# supports the apk, deb and docker datasources alone, and it reads an `ARG`
# only to resolve a variable inside a `FROM`. Checked against
# `lib/modules/manager/dockerfile/` on 2026-09-21. Tracking these needs a
# `customManagers` regex in the repository that builds the media. Recheck if
# that manager gains a releases datasource.
ARG INSTALLER_VERSION=0.1.2
ARG INSTALLER_SHA256_X86_64=248d49d28beb19647ece8a0729365e675e6423d83818cd26ffebedef38190c68
ARG INSTALLER_SHA256_AARCH64=7746831f2378c87f5258e8f51af48dd693b1dc3c3716038abf57c8b8a723ea56
RUN set -eux; \
    arch="$(uname -m)"; \
    case "$arch" in \
        x86_64) sha="${INSTALLER_SHA256_X86_64}";; \
        aarch64) sha="${INSTALLER_SHA256_AARCH64}";; \
        *) echo "the installer publishes no release for ${arch}" >&2; exit 1;; \
    esac; \
    asset="tect-installer-v${INSTALLER_VERSION}-${arch}-linux-gnu.tar.gz"; \
    curl --retry 3 -fsSLo /tmp/installer.tar.gz \
        "https://github.com/tectonic-os/installer/releases/download/v${INSTALLER_VERSION}/${asset}"; \
    echo "${sha}  /tmp/installer.tar.gz" | sha256sum -c -; \
    mkdir -p /out; \
    tar -xf /tmp/installer.tar.gz -C /out tect-installer

FROM ${LIVE_BASE}

# A sealed UKI payload keeps the boot files outside the sealed rootfs under
# `/kernel`; the media builder consumes their conventional module paths. This
# wrapper is not the installed payload, so exposing symlinks here changes
# neither the signed UKI nor the rootfs digest it carries.
RUN set -eux; \
    for split in /kernel/*; do \
        [ -d "$split" ] || continue; \
        kver="${split##*/}"; \
        for file in vmlinuz initramfs.img; do \
            [ ! -f "${split}/${file}" ] || [ -e "/usr/lib/modules/${kver}/${file}" ] \
                || ln -s "/kernel/${kver}/${file}" "/usr/lib/modules/${kver}/${file}"; \
        done; \
    done

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
#
# A UKI payload removes GRUB from the installed chain. The media still boots
# the vendor-signed shim and GRUB pair, restored only in this wrapper.
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
        if [ -s /usr/share/tectonic/boot-chain ] && [ ! -d /usr/lib/efi/grub2 ]; then \
            if ! dnf reinstall -y --setopt=install_weak_deps=False grub2-efi-x64; then \
                dnf upgrade -y --setopt=install_weak_deps=False grub2-efi-x64; \
            fi; \
            test -d /usr/lib/efi/grub2; \
        fi; \
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
#
# Neither of these autostarts the installer any more. The serial console is a
# root shell for watching a run, and `getty@tty1` is the last resort under both
# installer units below. The installer itself is a unit and owns tty1.
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
# The installer's own unit below runs it, so no `kmsconvt@` login is involved.
RUN systemctl enable var-lib-tectonic-store.mount

# The frontend the units below start. `--version` runs it here, so a binary
# that cannot execute in this environment fails the ISO build instead of the
# boot. That check means more since the binary stopped being the one that built
# the media: it is the net under `a user's own base`, whose live environment can
# carry an older glibc than the release was built against.
#
# `tect` itself is not staged. Nothing on this media runs it since the
# installer became its own binary, and the media carries one frontend.
COPY --from=tools /out/tect-installer /usr/bin/tect-installer
RUN /usr/bin/tect-installer --version

# The installer is this media's only job, so it is a unit on tty1 and not a
# login shell's profile. A unit cannot be started twice and the console it
# draws on is named here, so one installer per machine is structural rather
# than enforced after the fact. Nothing logs in on tty1 at all.
#
# Masking `getty@tty1` is what keeps the console; `Conflicts=` alone settles
# only the boot transaction, where it is what makes systemd drop the getty's
# start job rather than race it. logind spawns `autovt@` on a switch to an
# unused VT within `NAutoVTs`, 6 by default and so including tty1, and
# `autovt@.service` is a symlink to `getty@.service`. So Ctrl-Alt-F2 and back
# would start `getty@tty1`, whose conflict then stops the installer, and a stop
# systemd asked for is not one `Restart=` undoes: the installer would be gone
# until reboot, mid-install if it was installing. Both names are masked below,
# since `autovt@tty1` is its own unit and masking the getty does not cover it.
#
# `Restart=always` because leaving the installer on installation media starts it
# again rather than reaching a shell.
#
# The path here is the one the `COPY --from=tools` above writes. Nothing else
# ties a unit's `ExecStart=` to the binary this file stages, so an edit to one
# and not the other leaves the media booting to a login prompt, a root shell or
# a respawn loop on a blank tty1. The test
# `both_installer_units_hand_over_to_the_binary_the_live_environment_stages`
# reads each unit's own body and is the tie.
COPY <<'UNIT' /usr/lib/systemd/system/tect-installer.service
[Unit]
Description=Install this image onto a disk
After=var-lib-tectonic-store.mount systemd-user-sessions.service getty@tty1.service
Conflicts=getty@tty1.service
AssertPathExistsGlob=/dev/dri/card*
OnFailure=tect-installer-vt.service
StartLimitIntervalSec=60
StartLimitBurst=10

[Service]
Type=idle
ExecStart=/usr/bin/kmscon --vt=1 --no-switchvt --oneshot --login -- /usr/bin/tect-installer
Restart=always
RestartSec=1

[Install]
WantedBy=multi-user.target
UNIT

# `--oneshot` is load bearing. kmscon is a getty replacement and respawns its
# login process in place by default, so without it `tect-installer` exiting is
# invisible to systemd: the unit stays active, `Restart=` never runs, the start
# limit is never reached and the fallback below is unreachable. With it, kmscon
# exits when the installer does and systemd owns the restart.
#
# The assertion decides the fallback, not the start limit. No DRM device is the
# case this falls back for, and an assertion failure enters `failed` where a
# condition failure would quietly skip. The limit is left to catch a kmscon that
# fails some other way, and is loose enough that a person answering `Quit the
# installer` repeatedly does not trip it. If they do, the cost is this unit
# instead: the installer on the kernel's own VT, losing the box-drawing arcs.
#
# This one never hands over on its own. `StartLimitIntervalSec=0` because
# leaving the installer starts it again, and a rate limit here would answer a
# person's fifth `Quit` with a root shell on the console. The action row's
# `Exit to shell` is the deliberate route to one, and it puts this unit back
# when the shell exits. Recovery from an installer that cannot run is
# Ctrl-Alt-F2 or the serial console, both of which autologin root.
COPY <<'UNIT' /usr/lib/systemd/system/tect-installer-vt.service
[Unit]
Description=Install this image onto a disk, on the kernel console
After=var-lib-tectonic-store.mount systemd-user-sessions.service getty@tty1.service
Conflicts=getty@tty1.service
StartLimitIntervalSec=0

[Service]
Type=idle
ExecStart=/usr/bin/tect-installer
Restart=always
RestartSec=1
StandardInput=tty
StandardOutput=tty
TTYPath=/dev/tty1
TTYReset=yes
TTYVHangup=yes

[Install]
WantedBy=multi-user.target
UNIT

# One of the two is enabled, never both. The apt arm of this file has no kmscon
# package, and the dnf arm asks for it with `|| true`, so an EL base or a repo
# that was down lands on the fallback too.
RUN set -eux; \
    systemctl mask getty@tty1.service autovt@tty1.service; \
    if command -v kmscon > /dev/null 2>&1; then \
        systemctl enable tect-installer.service; \
    else \
        systemctl enable tect-installer-vt.service; \
    fi
