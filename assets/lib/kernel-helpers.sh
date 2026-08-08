#!/bin/bash

kver() {
    local pkg
    pkg="$(cat /usr/lib/kernel-build/kernel-package 2> /dev/null || echo 'kernel-core')"
    rpm -q --qf '%{VERSION}-%{RELEASE}.%{ARCH}' "$pkg"
}
