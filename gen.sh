#!/bin/bash

# For each of the listed version, check out relevant Rust version and calculate
# submodule updates, including submodule commits and whether it's an orphan commit or not.

VERSIONS=(1.17.0 1.18.0 1.19.0 1.20.0 1.21.0 1.22.0 1.23.0 1.24.0 1.25.0 1.26.0 1.27.0 1.28.0 1.29.0 1.30.0 1.31.0 1.31.1 1.32.0 1.33.0 1.34.0 1.35.0)

git --git-dir=/home/xanewok/repos/rust fetch upstream

for version in "${VERSIONS[@]}"; do
    cd ~/repos/rust
    git checkout $version
    cd ~/repos/subhistory
    cargo run --release > out.$version.txt
done

