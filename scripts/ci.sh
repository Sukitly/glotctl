#!/bin/sh
set -e

cargo fmt --all
cargo clippy --all-targets --all-features
cargo build --all-targets
cargo test
