# glot

A fast CLI for checking internationalization (i18n) issues in Next.js projects using [next-intl](https://next-intl.dev/).

📖 **[Full Documentation](https://glotctl.mintlify.app/)**

## Features

- 🔍 **Hardcoded Text Detection** - Find untranslated text in JSX/TSX
- 🌐 **Untranslated Detection** - Detect values identical to primary locale
- 🔑 **Missing Key Detection** - Identify keys used in code but missing from locale files
- 🧹 **Orphan Key Detection** - Find unused keys in locale files
- 🤖 **AI Integration** - MCP server for AI coding agents

## Installation

```bash
npm install -D glotctl
```

> The npm package is `glotctl`, but the CLI command is `glot`.

## Quick Start

```bash
# Initialize configuration
npx glot init

# Check for all i18n issues
npx glot check
```

## What Glot Detects

### Hardcoded Text

Untranslated strings in JSX that should use translation functions:

```tsx
// ❌ Detected by glot
<button>Submit</button>
<input placeholder="Enter email" />

// ✅ Using next-intl
<button>{t("submit")}</button>
<input placeholder={t("emailPlaceholder")} />
```

```
error: "Submit"  hardcoded-text
  --> ./src/components/Button.tsx:5:22
  |
5 |     return <button>Submit</button>;
  |                    ^
```

### Missing Keys

Translation keys used in code but not defined in locale files:

```tsx
// Code uses this key
const t = useTranslations("common");
return <button>{t("submit")}</button>;
```

```jsonc
// messages/en.json - key is missing!
{
  "common": {
    "cancel": "Cancel"
  }
}
```

```
error: common.submit  missing-key
  --> ./src/components/Button.tsx:3
  |
  | Translation key "common.submit" is used but not defined
```

### Orphan Keys

Keys defined in locale files but never used in code:

```jsonc
// messages/en.json
{
  "common": {
    "submit": "Submit",
    "oldButton": "Old Text" // Never used
  }
}
```

```
warning: common.oldButton  orphan-key
  --> ./messages/en.json
  |
  | Key exists in locale file but is not used in code
```

### Untranslated Values

Values in non-primary locales that are identical to the primary locale, possibly not translated:

```jsonc
// messages/en.json (primary)
{
  "common": {
    "submit": "Submit"
  }
}

// messages/zh.json - same as English!
{
  "common": {
    "submit": "Submit"
  }
}
```

```
warning: common.submit  untranslated
  --> ./messages/zh.json:3:0
  = note: "Submit"
  = hint: Value is identical to primary locale (en), possibly not translated
```

Clean up orphan keys:

```bash
npx glot clean         # Preview
npx glot clean --apply # Apply
```

## Existing Projects

For projects with many existing hardcoded strings, use `baseline` to suppress current warnings and prevent new ones:

```bash
npx glot baseline         # Preview
npx glot baseline --apply # Apply
```

This inserts `// glot-disable-next-line` comments, allowing you to:

1. Add glot to CI immediately
2. Gradually fix existing issues over time

## AI Integration (MCP)

Glot provides an [MCP server](https://modelcontextprotocol.io/) for AI coding agents.

### OpenCode

Add to `opencode.json`:

```json
{
  "mcp": {
    "glot": {
      "type": "local",
      "command": ["npx", "glot", "serve"],
      "enabled": true
    }
  }
}
```

### Claude Code

```bash
claude mcp add --transport stdio glot -- npx glot serve
```

Or create `.mcp.json` in your project root:

```json
{
  "mcpServers": {
    "glot": {
      "command": "npx",
      "args": ["glot", "serve"]
    }
  }
}
```

### Cursor

Create `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "glot": {
      "command": "npx",
      "args": ["glot", "serve"]
    }
  }
}
```

See [MCP Server Documentation](https://glotctl.mintlify.app/agents/mcp) for available tools and workflow.

## License

MIT
