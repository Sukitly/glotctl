# Clean Example

This example demonstrates **proper next-intl usage** with zero i18n issues. All text is properly internationalized.

## What's Correct

### ✓ No Hardcoded Text
- All JSX text uses `t()` function
- All attributes (placeholder, aria-label, title) use translations
- Conditional expressions use translated values

### ✓ All Keys Exist
- Every `t('key')` call has a corresponding entry in `messages/en.json`

### ✓ Static Keys Only
- No dynamic key construction
- All translation keys are string literals

### ✓ Complete Translations
- Chinese locale (`zh.json`) has all keys from English locale
- No replica-lag issues

### ✓ No Unused Keys
- Every key in message files is used in the code

### ✓ No Orphan Keys
- No keys exist only in non-primary locales

### ✓ Properly Translated
- All Chinese translations are different from English (no untranslated values)

### ✓ Consistent Types
- All values are strings (no type mismatches between locales)

### ✓ Valid Syntax
- All files parse correctly

## Usage

```bash
# From repo root
cargo build
cd examples/clean

# Check (should report zero issues)
../../target/debug/glot check

# Verbose output (should show "No issues found")
../../target/debug/glot check -v

# Using justfile (from repo root)
just test-example clean
```

## Expected Output

```
✓ Checked 3 source files, 2 locale files - no issues found
```

Exit code: 0

## Use Cases

- **Baseline testing**: Ensure glot doesn't report false positives
- **Reference implementation**: See how to properly structure next-intl projects
- **Documentation**: Examples for best practices guide
- **Regression testing**: Verify new features don't break on clean code
