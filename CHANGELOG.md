# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-01-07

### Added

- **Hardcoded Text Detection**: Find untranslated text in JSX/TSX files
  - Supports JSX text nodes, string literals, and template literals
  - Detects hardcoded text in JSX attributes (placeholder, title, alt, aria-*)
  - Unicode-aware detection supporting all languages

- **Missing Key Detection**: Identify translation keys used in code but missing from locale files
  - Supports `useTranslations` hook with various namespace patterns
  - Handles dynamic keys with static analysis
  - Schema factory function support for nested namespaces

- **Orphan Key Detection**: Find unused translation keys in locale files
  - Cross-references code usage with locale file definitions

- **Replica Lag Detection**: Find keys in primary locale missing from other locales
  - Helps maintain translation parity across all locales

- **Inline Directives**: Suppress specific issues using comments
  - `glot-disable-next-line` for single line suppression
  - `glot-disable` / `glot-enable` for block suppression
  - Works in both JavaScript and JSX comment styles

- **Baseline Support**: Insert suppression comments for existing issues
  - `glot baseline` for dry-run preview
  - `glot baseline --apply` to insert comments

- **Clean Command**: Remove unused translation keys
  - `glot clean` for dry-run preview
  - `glot clean --apply` to remove keys
  - Options for `--unused` and `--orphan` key types

- **MCP Server**: Model Context Protocol integration for AI coding agents
  - `get_config`: Retrieve current configuration
  - `get_locales`: List available locale files
  - `scan_overview`: Get statistics of all i18n issues
  - `scan_hardcoded`: Paginated hardcoded text list
  - `scan_primary_missing`: Paginated missing keys list
  - `scan_replica_lag`: Paginated replica lag list
  - `add_translations`: Add keys to locale files

- **Configuration**: Flexible `.glotrc.json` configuration
  - Configurable primary locale
  - Custom include/ignore patterns
  - Configurable checked attributes
  - Test file ignoring option

### Technical

- Built with Rust 2024 edition
- Uses SWC parser (same as Next.js) for accurate TSX/JSX parsing
- AST visitor pattern for efficient code traversal
- Snapshot testing with insta for CLI behavior verification

[Unreleased]: https://github.com/Sukitly/glot/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Sukitly/glot/releases/tag/v0.1.0
