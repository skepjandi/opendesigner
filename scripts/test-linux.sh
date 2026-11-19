#!/usr/bin/env bash
# Builds and tests OpenDesigner on Linux in a container (Docker): the same checks as AGENTS.md
# "Testing", for the default workspace members and the Qt shell. The container's cargo target
# directory is a named volume, so a second run is incremental.
#
#   scripts/test-linux.sh            # everything
#   scripts/test-linux.sh <command>  # one command inside the container, for example: cargo test -p od-render
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
image="opendesigner-linux"

docker build -q -t "$image" -f "$root/packaging/linux/Dockerfile" "$root/packaging/linux" >/dev/null

if [ "$#" -gt 0 ]; then
    steps="$*"
else
    steps='cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo clippy -p opendesigner-app --all-targets --locked -- -D warnings
cargo test -p opendesigner-app --locked'
fi

docker run --rm \
    -v "$root":/work \
    -v opendesigner-linux-target:/work/target \
    -v opendesigner-linux-cargo:/opt/cargo/registry \
    "$image" bash -euxc "$steps"
