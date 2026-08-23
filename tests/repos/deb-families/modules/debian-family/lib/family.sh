#!/bin/bash

install_packages() {
    apt-get update
    apt-get install -y "$@"
}
