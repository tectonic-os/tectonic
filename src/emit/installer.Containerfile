# The installer live environment. `tect vm build iso` stages this into
# out/bootiso/ and sets `LIVE_BASE` to the payload itself for a dnf family; the
# Fedora default serves the rest, whose images tacklebox stages unsigned.

# renovate: datasource=docker depName=docker.io/library/golang
ARG GO_IMAGE=docker.io/library/golang:1.26
# renovate: datasource=docker depName=quay.io/fedora/fedora-bootc
ARG LIVE_BASE=quay.io/fedora/fedora-bootc:44

# Tacklebox publishes no binary worth pinning, so its source archive is pinned
# by sha256 and built here.
# CGO_ENABLED=0 because tacklebox's default build links `net` and `os/user`
# against the builder's libc, and it is copied out of this stage to a host.
FROM ${GO_IMAGE} AS tools
# Tacklebox is the other fork: the media needs a change upstream has not got.
# The pin is `feat/offline-store-format`, stacked on the GRUB support that
# stages the live image's signed shim and GRUB pair. `vm.sh` asks for GRUB and
# an OCI store through the two switches carried by the fork.
ARG TACKLEBOX_ORG=tectonic-os
ARG TACKLEBOX_COMMIT=a536788bac2eaeb1403cb2855a7f3b4bc2a1a029
ARG TACKLEBOX_SHA256=9116d68d4bb2030398a1759ac6c029a0f4d8a2edb513e018a54aeaaf144e5ebc
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
    fetch "${TACKLEBOX_ORG}" tacklebox "${TACKLEBOX_COMMIT}" "${TACKLEBOX_SHA256}"; \
    git -C /src/tacklebox apply -p1 /tmp/efi-from-image.patch; \
    mkdir -p /out; \
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
# These three are bumped by hand, as the tacklebox pins above are. A
# `# renovate:` directive here would do nothing: the dockerfile manager
# supports the apk, deb and docker datasources alone, and it reads an `ARG`
# only to resolve a variable inside a `FROM`, as
# `lib/modules/manager/dockerfile/` shows. Tracking these needs a
# `customManagers` regex in the repository that builds the media.
ARG INSTALLER_VERSION=0.1.5
ARG INSTALLER_SHA256_X86_64=ee8f0548545378e66161f427f514928cf35683b916b591550108f0cb8c42b7e8
ARG INSTALLER_SHA256_AARCH64=0c2e3f6da8d13579d3d9e2b1c644cbde7611fdc0c28225a727105d336c488b29
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
# the family installed and there is one live environment. This asserts the
# binaries the installer runs and never the packages that happen to provide
# them.
#
# openssl makes the password hash before the disk is changed. crypt(3) is not
# an option because glibc keeps it in libcrypt.
#
# The apt arm produces unsigned media. EL packages no kmscon and no
# btrfs-progs, so each is asked for alone. The installer refuses a btrfs
# format before the cut where the live environment has no mkfs.btrfs. The
# assertion below makes a wrong package name a failed ISO build and not a
# wiped disk.
#
# A UKI payload removes GRUB from the installed chain. The media still boots
# the vendor-signed shim and GRUB pair, restored only in this wrapper.
RUN set -eux; \
    if command -v apt-get > /dev/null 2>&1; then \
        apt-get update -y; \
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            podman cryptsetup openssl fdisk util-linux passwd policycoreutils kbd \
            dosfstools e2fsprogs xfsprogs btrfs-progs; \
        apt-get clean -y; \
        rm -rf /var/lib/apt/lists/*; \
    else \
        dnf install -y --setopt=install_weak_deps=False \
            podman cryptsetup openssl util-linux shadow-utils policycoreutils kbd \
            dosfstools e2fsprogs xfsprogs; \
        if [ -s /usr/share/tectonic/boot-chain ] && [ ! -d /usr/lib/efi/grub2 ]; then \
            if ! dnf reinstall -y --setopt=install_weak_deps=False grub2-efi-x64; then \
                dnf upgrade -y --setopt=install_weak_deps=False grub2-efi-x64; \
            fi; \
            test -d /usr/lib/efi/grub2; \
        fi; \
        dnf install -y --setopt=install_weak_deps=False kmscon || true; \
        dnf install -y --setopt=install_weak_deps=False btrfs-progs || true; \
        dnf clean all; \
    fi; \
    for tool in podman cryptsetup openssl sfdisk lsblk blkid losetup mount umount mountpoint \
        chroot useradd setfiles systemctl chvt mkfs.fat mkfs.ext4 mkfs.xfs; do \
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

# The recipe is baked in at a fixed path because this layer is where the target
# is already known.
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

# The live graphroot cannot use overlay on the live image's own overlay root.
COPY <<'MOUNT' /usr/lib/systemd/system/var-lib-containers-storage.mount
[Unit]
Description=Temporary container storage for the installer

[Mount]
What=tmpfs
Where=/var/lib/containers/storage
Type=tmpfs
Options=mode=0700

[Install]
WantedBy=multi-user.target
MOUNT

# The installer bind-mounts this configuration into the payload container, so
# every configured helper must exist there as well as in the live environment.
COPY <<'CONF' /etc/containers/storage.conf
[storage]
driver = "overlay"
runroot = "/run/containers/storage"
graphroot = "/var/lib/containers/storage"

[storage.options]
additionalimagestores = ["/var/lib/tectonic/store"]
CONF

# Root on the console, without a password, on installer media only. The
# installer partitions disks and calls `bootc install`, so a console that
# cannot become root cannot install anything. This image is built per target as
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
RUN systemctl enable var-lib-tectonic-store.mount var-lib-containers-storage.mount

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
# kmscon's `--login` starts its command with no `PATH`, and the installer runs
# `mkfs.*` from `/usr/sbin`, so `env` passes the login PATH a root console has.
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
Requires=var-lib-containers-storage.mount
After=var-lib-tectonic-store.mount var-lib-containers-storage.mount systemd-user-sessions.service getty@tty1.service
Conflicts=getty@tty1.service
AssertPathExistsGlob=/dev/dri/card*
OnFailure=tect-installer-vt.service
StartLimitIntervalSec=60
StartLimitBurst=10

[Service]
Type=idle
ExecStart=/usr/bin/kmscon --vt=1 --no-switchvt --oneshot --login -- /usr/bin/env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin /usr/bin/tect-installer
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
Requires=var-lib-containers-storage.mount
After=var-lib-tectonic-store.mount var-lib-containers-storage.mount systemd-user-sessions.service getty@tty1.service
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
