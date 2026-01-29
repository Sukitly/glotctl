# Glot Examples

This directory contains example Next.js projects for manually testing and demonstrating glot functionality.

## Available Examples

### 1. [all-issues/](./all-issues/)
**Comprehensive issue showcase** - Contains all 9 types of i18n issues that glot can detect.

Use this for:
- Verifying all detection types work correctly
- Testing verbose output
- Documentation screenshots
- Demonstrating the full range of glot capabilities

```bash
just test-example all-issues
```

### 2. [clean/](./clean/)
**Best practices reference** - Zero issues, demonstrates proper next-intl usage.

Use this for:
- Baseline testing (ensuring no false positives)
- Reference implementation for best practices
- Regression testing

```bash
just test-example clean
# Should exit with code 0 and report no issues
```

### 3. [real-world/](./real-world/)
**Realistic mixed scenario** - Some issues, some correct usage, includes suppression comments.

Use this for:
- Testing suppression comments (`glot-disable-next-line`, `glot-disable`, `glot-enable`)
- Testing dynamic key annotations (`glot-message-keys`)
- Simulating gradual migration workflows
- Testing configuration options (ignores)

```bash
just test-example real-world
```

## Quick Start

```bash
# Build glot first
cargo build

# Test all examples at once
just test-examples

# Test a specific example
just test-example all-issues
just test-example clean
just test-example real-world

# Test with verbose output
just test-example-v all-issues
```

## Testing Commands

Each example can be used to test different glot commands. **Note:** Commands must be run from within the example directory.

### Check Command
```bash
# Check all issue types (from example directory)
cd examples/all-issues
../../target/debug/glot check

# Check specific issue type
../../target/debug/glot check hardcoded
../../target/debug/glot check missing

# Verbose output
cd examples/clean
../../target/debug/glot check -v
```

### Baseline Command
Add suppression comments for all detected issues:
```bash
cd examples/all-issues
../../target/debug/glot baseline --apply
```

### Fix Command
Add `glot-message-keys` annotations for unresolved dynamic keys:
```bash
cd examples/all-issues
../../target/debug/glot fix --apply
```

### Clean Command
Remove unused and orphan keys from message files:
```bash
cd examples/real-world
../../target/debug/glot clean --apply
```

## Example Structure

Each example follows this structure:
```
example-name/
├── src/
│   ├── components/     # React components
│   └── app/            # Next.js app directory
├── messages/
│   ├── en.json         # Primary locale
│   ├── zh.json         # Secondary locale(s)
│   └── ...
├── .glotrc.json        # Glot configuration
└── README.md           # Example-specific documentation
```

## Issue Types Covered

| Issue Type | all-issues | clean | real-world |
|------------|------------|-------|------------|
| hardcoded | ✓ | - | ✓ (some suppressed) |
| missing-key | ✓ | - | - |
| unresolved-key | ✓ | - | - (with annotations) |
| replica-lag | ✓ | - | - |
| unused-key | ✓ | - | ✓ |
| orphan-key | ✓ | - | - |
| untranslated | ✓ | - | - |
| type-mismatch | ✓ | - | - |
| parse-error | ✓ | - | - |

## Adding New Examples

When adding new examples:
1. Follow the directory structure above
2. Include a comprehensive README.md
3. Add configuration in `.glotrc.json`
4. Update this README.md with the new example
5. Add to `.gitignore` if needed (node_modules, etc.)

## CI Integration

Examples can be used in CI to prevent regressions:

```bash
# In CI pipeline
cargo build
./target/debug/glot check examples/clean  # Should pass
./target/debug/glot check examples/all-issues  # Should fail with specific issues
```

## Notes

- Examples are **not** full Next.js applications (no package.json, no node_modules)
- They contain only the minimum files needed to demonstrate glot functionality
- Focus is on TSX/JSX files and message JSON files
- Future: May add edge-cases/ or monorepo/ examples as needed
