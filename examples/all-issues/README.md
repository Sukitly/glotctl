# All Issues Example

This example contains ALL 9 types of i18n issues that glot can detect.

## Issues Included

### 1. **hardcoded** (Error)
- Location: `src/components/Hardcoded.tsx`
- Examples: JSX text, attributes (placeholder, alt, title), conditionals, logical expressions, template literals

### 2. **missing-key** (Error)
- Location: `src/components/MissingKeys.tsx`
- Keys used in code but not defined in `messages/en.json`:
  - `nonexistent.title`
  - `missing.description`
  - `actions.submit`

### 3. **unresolved-key** (Warning)
- Location: `src/components/UnresolvedKeys.tsx`
- Dynamic keys that can't be resolved statically:
  - `dynamic.${dynamicKey}`
  - `roles.${userRole}.name`
  - Function call: `getDynamicKey()`

### 4. **replica-lag** (Error)
- Location: `messages/zh.json`
- Missing "form" namespace (present in primary locale `en.json`)
- Missing "valid" namespace

### 5. **unused-key** (Warning)
- Location: `messages/en.json`
- Keys defined but never used:
  - `common.unused`
  - `common.another_unused`

### 6. **orphan-key** (Warning)
- Location: `messages/es.json`
- Keys in non-primary locale but not in primary:
  - `orphan.only_in_spanish`
- Location: `messages/zh.json`
- Keys in non-primary locale but not in primary:
  - `legacy.old`

### 7. **untranslated** (Warning)
- Location: `messages/zh.json`
- Identical value to English:
  - `common.button` (both "Submit")

### 8. **type-mismatch** (Error)
- Location: `messages/zh.json`
- `common.unused` is an array in zh.json but a string in en.json

### 9. **parse-error** (Error)
- Location: `src/components/ParseError.tsx`
- Invalid JSX syntax (missing closing tag)

## Usage

```bash
# From repo root
cargo build
cd examples/all-issues

# Check all issues
../../target/debug/glot check

# Check specific issue types
../../target/debug/glot check hardcoded
../../target/debug/glot check missing
../../target/debug/glot check replica-lag

# Verbose output
../../target/debug/glot check -v

# Using justfile (from repo root)
just test-example all-issues
just test-example-v all-issues
```

## Expected Output

Should report **31 total problems** (19 errors, 12 warnings) across all 9 types:
- **10 hardcoded errors** (in Hardcoded.tsx)
- **3 missing-key errors** (in MissingKeys.tsx)
- **3 unresolved-key warnings** (in UnresolvedKeys.tsx)
- **3 replica-lag errors** (form.*, valid.key, common.another_unused)
- **6 unused-key warnings** (common.*, form.*, valid.key)
- **2 orphan-key warnings** (orphan.only_in_spanish, legacy.old)
- **1 untranslated warning** (common.button)
- **1 type-mismatch error** (common.unused)
- **1 parse-error** (ParseError.tsx)

## Testing Other Commands

```bash
# Add baseline suppressions
../../target/debug/glot baseline --apply

# Add message-keys annotations for unresolved keys
../../target/debug/glot fix --apply

# Remove unused/orphan keys
../../target/debug/glot clean --apply
```
