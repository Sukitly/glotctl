# glot

A fast CLI tool for checking internationalization (i18n) issues in Next.js projects using [next-intl](https://next-intl.dev/).

📖 **[Full Documentation](https://glotctl.mintlify.app/)**

## Features

- 🔍 **Hardcoded Text Detection** - Find untranslated text in JSX/TSX files
- 🔑 **Missing Key Detection** - Identify keys used in code but missing from locale files
- 🧹 **Orphan Key Detection** - Find unused keys in locale files
- 🤖 **AI Integration** - MCP server for AI coding agents

## Quick Example

```tsx
// ❌ Detected by glot
<button>Submit</button>
<input placeholder="Enter email" />

// ✅ Using next-intl
<button>{t("submit")}</button>
<input placeholder={t("emailPlaceholder")} />
```

## Installation

```bash
npm install -D glotctl
```

> The npm package is `glotctl`, but the CLI command is `glot`.

## Quick Start

```bash
# Initialize configuration
npx glot init

# Check for i18n issues
npx glot check

# Clean unused keys (preview)
npx glot clean

# Clean unused keys (apply)
npx glot clean --apply
```

## License

MIT
