# Run local CI checks (format, lint, build, test)
ci:
    cargo fmt --all
    cargo clippy --all-targets --all-features
    cargo build --all-targets
    cargo test
