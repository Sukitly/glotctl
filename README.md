# glot

Find hardcoded strings and missing i18n keys in your Next.js app.

## Install

```bash
npm install -D glotctl
npx glot init
npx glot check
```

## What it catches

**Hardcoded text** — forgot to use `t()`

```
error: "Submit"  hardcoded-text
  --> src/components/Button.tsx:3:22
  |
3 |       return <button>Submit</button>;
  |                      ^^^^^^
```

**Missing keys** — used in code but not in locale files

```
error: "Common.submit"  missing-key
  --> src/app.tsx:4:23
  |
4 |       return <button>{t("submit")}</button>;
  |                       ^^^^^^^^^^^
```

**Orphan keys** — exists in some locales but not others

```
warning: "Common.oldKey"  orphan-key
  --> messages/es.json:1:0
  = note: in es ("Enviar")
```

## AI Agents

Works as an MCP server for Claude, Cursor, etc:

```bash
npx glot serve
```

## Docs

https://glotctl.mintlify.app/

## License

MIT
