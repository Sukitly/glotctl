# Real-World Example

This example demonstrates a **realistic mixed scenario** with both correct usage and some issues, plus advanced features like suppression comments and dynamic key annotations.

## What's Included

### Correct Usage
- `src/app/page.tsx` - Proper translations
- `src/components/Header.tsx` - Mix of correct and hardcoded text
- `src/components/Legacy.tsx` - Suppressed hardcoded text for gradual migration
- `src/components/Dynamic.tsx` - Dynamic keys with `glot-message-keys` annotations

### Issues Present

**Hardcoded Text** (some suppressed):
- `src/components/Header.tsx:9` - "Sign Out" button (not suppressed)
- `src/components/Header.tsx:15` - "Search..." placeholder (not suppressed)
- `src/components/Legacy.tsx:7-10` - Suppressed with `glot-disable-next-line`
- `src/components/Legacy.tsx:16-19` - Suppressed with `glot-disable` / `glot-enable`

**Unused Keys**:
- `legacy.unused_old_key` - Present in both en.json and zh.json but never used

### Advanced Features

#### 1. Suppression Comments
```tsx
// Suppress single line
{/* glot-disable-next-line hardcoded */}
<h1>Legacy Header</h1>

// Suppress multiple lines
{/* glot-disable */}
<div>
  <span>Old Component</span>
  <span>Will be refactored soon</span>
</div>
{/* glot-enable */}
```

#### 2. Dynamic Key Annotations
```tsx
{/* glot-message-keys "dynamic.roles.admin.name" "dynamic.roles.editor.name" "dynamic.roles.viewer.name" */}
<p>{t(`roles.${userRole}.name`)}</p>
```

#### 3. Configuration with Ignores
```json
{
  "ignores": [
    "**/*.test.tsx",
    "**/*.spec.tsx"
  ]
}
```

## Usage

```bash
# From repo root
cargo build
cd examples/real-world

# Check all issues
../../target/debug/glot check

# Check only hardcoded (should show 2 unsuppressed)
../../target/debug/glot check hardcoded

# Check unused keys
../../target/debug/glot check unused

# Verbose output
../../target/debug/glot check -v

# Using justfile (from repo root)
just test-example real-world
```

## Expected Output

Should report **3 problems** (2 errors, 1 warning):
- **2 hardcoded errors** (unsuppressed ones in Header.tsx: "Sign Out", "Search...")
- **1 unused-key warning** (legacy.unused_old_key)
- **0 unresolved-key issues** (thanks to glot-message-keys annotations)

The suppressed hardcoded text in Legacy.tsx should NOT be reported.

## Testing Commands

### Baseline Command
Add suppression comments for all issues:
```bash
../../target/debug/glot baseline --apply
```

This should add `glot-disable-next-line` comments above the hardcoded text in Header.tsx.

### Fix Command
Add message-keys annotations for dynamic keys:
```bash
../../target/debug/glot fix --apply
```

This would add annotations if any unresolved keys were detected.

### Clean Command
Remove unused/orphan keys:
```bash
../../target/debug/glot clean --apply
```

This should remove `legacy.unused_old_key` from both message files.

## Workflow Simulation

This example is perfect for testing a realistic migration workflow:

1. **Initial check**: Find hardcoded text and unused keys
2. **Apply baseline**: Suppress issues you'll fix later
3. **Gradual migration**: Fix issues one by one
4. **Remove suppressions**: As you migrate, remove the comments
5. **Clean up**: Remove unused keys when migration is complete

## Use Cases

- **Gradual migration**: Shows how to use suppressions during incremental refactoring
- **Dynamic keys**: Demonstrates proper handling of runtime-determined keys
- **Configuration**: Shows how to set up ignores and other options
- **Realistic testing**: Mix of issues reflects actual codebases
