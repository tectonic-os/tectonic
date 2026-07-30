#!/bin/bash

set -ouex pipefail

systemctl mask systemd-remount-fs.service

mv /opt /opt.bak
mkdir /opt

mv /usr/bin/systemctl /usr/bin/systemctl.bak
ln -s /usr/bin/true /usr/bin/systemctl

dnf5 install -y dnf5-plugins

dnf5 install -y checkpolicy
