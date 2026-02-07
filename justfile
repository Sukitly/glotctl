# Run local CI checks (format, lint, build, test)
ci:
    cargo fmt --all
    cargo clippy --all-targets --all-features
    cargo build --all-targets
    cargo test

# Test all examples
test-examples:
    cargo build
    @echo "Testing all-issues..."
    cd examples/all-issues && ../../target/debug/glot check
    @echo "\nTesting clean (should pass)..."
    cd examples/clean && ../../target/debug/glot check
    @echo "\nTesting real-world..."
    cd examples/real-world && ../../target/debug/glot check

# Test specific example
test-example name:
    cargo build && cd examples/{{name}} && ../../target/debug/glot check

# Test with verbose
test-example-v name:
    cargo build && cd examples/{{name}} && ../../target/debug/glot check -v

# Release a new version (patch, minor, or major)
release level="patch":
    ./scripts/release.sh {{level}}
