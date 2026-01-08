# Contributing to Glot

Thank you for your interest in contributing to Glot! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust 2024 edition (latest stable)
- Cargo

### Getting Started

1. Clone the repository:

   ```bash
   git clone https://github.com/Sukitly/glot.git
   cd glot
   ```

2. Build the project:

   ```bash
   cargo build
   ```

3. Run the tests:
   ```bash
   cargo test
   ```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and address any warnings
- Follow standard Rust naming conventions

## Running Tests

```bash
# Run all tests
cargo test

# Run a specific test
cargo test test_name

# Run CLI integration tests
cargo test --test '*'

# Update insta snapshots (for CLI tests)
cargo insta test --accept
```

## Project Structure

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Library entry, orchestrates commands
├── args.rs              # CLI argument definitions
├── config.rs            # Configuration management
├── file_scanner.rs      # Directory traversal and file filtering
├── issue.rs             # Issue type definitions
├── reporter.rs          # Output formatting (cargo-style diagnostics)
├── json_editor.rs       # JSON file editing utilities
├── json_writer.rs       # JSON file writing utilities
├── checkers/            # Core checking logic
│   ├── hardcoded.rs     # Hardcoded text detection
│   ├── missing_keys/    # Missing key detection
│   ├── glob_matcher.rs  # Glob pattern matching
│   ├── schema.rs        # Schema factory analysis
│   ├── translation_calls.rs  # Translation function call detection
│   └── ...
├── commands/            # Command implementations
│   ├── check.rs         # Check command
│   ├── clean.rs         # Clean command
│   ├── baseline.rs      # Baseline command
│   ├── context.rs       # Command context
│   ├── runner.rs        # Command runner
│   └── shared.rs        # Shared utilities
├── rules/               # Rule implementations
│   ├── hardcoded.rs     # Hardcoded text rule
│   ├── missing.rs       # Missing key rule
│   └── orphan.rs        # Orphan key rule
├── parsers/             # File parsing utilities
│   ├── jsx.rs           # JSX/TSX parsing with SWC
│   ├── json.rs          # JSON locale file parsing
│   └── comment.rs       # Directive comment parsing
└── mcp/                 # MCP server implementation
    ├── server.rs        # MCP server entry
    ├── helpers.rs       # MCP helper functions
    └── types.rs         # MCP type definitions
```

## Making Changes

### Before You Start

1. Check existing issues and PRs to avoid duplicate work
2. For significant changes, open an issue first to discuss the approach
3. Keep changes focused - one feature or fix per PR

### Commit Messages

- Use clear, descriptive commit messages
- Start with a verb in imperative mood (e.g., "Add", "Fix", "Update")
- Reference issue numbers when applicable

Examples:

```
Add support for custom ignore patterns
Fix Unicode handling in JSX text detection
Update SWC parser to v30.0.0
```

### Pull Request Process

1. Create a feature branch from `main`
2. Make your changes with appropriate tests
3. Ensure all tests pass: `cargo test`
4. Ensure code is formatted: `cargo fmt`
5. Ensure no clippy warnings: `cargo clippy`
6. Submit a pull request with a clear description

## Testing Guidelines

- Add unit tests for new functionality in the same file using `#[cfg(test)]`
- Add integration tests in `tests/cli/` for CLI behavior changes
- Use `insta` for snapshot testing CLI output
- Ensure tests cover edge cases and error conditions

### Example Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_behavior() {
        // Arrange
        let input = "test input";

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected_value);
    }
}
```

## Reporting Issues

When reporting bugs, please include:

1. Glot version (`glot --version`)
2. Rust version (`rustc --version`)
3. Operating system
4. Steps to reproduce
5. Expected behavior
6. Actual behavior
7. Relevant configuration (`.glotrc.json`)

## Architecture Notes

### AST Parsing

Glot uses SWC (the same parser as Next.js) for parsing TSX/JSX files. The key pattern is the `Visit` trait:

```rust
impl Visit for Checker {
    fn visit_jsx_text(&mut self, node: &JSXText) {
        // Process JSX text nodes
    }
}
```

### Detection Rules

Text is reported if it contains at least one Unicode alphabetic character (`char::is_alphabetic()`), supporting all languages while ignoring pure numbers and symbols.

## License

By contributing to Glot, you agree that your contributions will be licensed under the MIT License.
