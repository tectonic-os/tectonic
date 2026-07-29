#!/bin/bash

set -ouex pipefail

FLAVOUR="${FLAVOUR:?}"

source /ctx/lib/brand-helpers.sh
brand_os_release
