# Code Examples

Detailed examples for fixing i18n issues with glot.

## Complete Component Transformation

### Before (with hardcoded text)

```tsx
// src/components/LoginForm.tsx
"use client";

import { useState } from "react";

export function LoginForm() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    // ... login logic
  };

  return (
    <form onSubmit={handleSubmit}>
      <h1>Sign In</h1>
      <p>Welcome back! Please enter your credentials.</p>

      {error && <div className="error">{error}</div>}

      <label>
        Email
        <input
          type="email"
          placeholder="Enter your email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
      </label>

      <label>
        Password
        <input
          type="password"
          placeholder="Enter your password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>

      <button type="submit" disabled={isLoading}>
        {isLoading ? "Signing in..." : "Sign In"}
      </button>

      <p>
        Don't have an account? <a href="/register">Create one</a>
      </p>
    </form>
  );
}
```

### After (with translations)

```tsx
// src/components/LoginForm.tsx
"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";

export function LoginForm() {
  const t = useTranslations("auth.login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    // ... login logic
  };

  return (
    <form onSubmit={handleSubmit}>
      <h1>{t("title")}</h1>
      <p>{t("subtitle")}</p>

      {error && <div className="error">{error}</div>}

      <label>
        {t("emailLabel")}
        <input
          type="email"
          placeholder={t("emailPlaceholder")}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
      </label>

      <label>
        {t("passwordLabel")}
        <input
          type="password"
          placeholder={t("passwordPlaceholder")}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>

      <button type="submit" disabled={isLoading}>
        {isLoading ? t("signingIn") : t("signInButton")}
      </button>

      <p>
        {t("noAccount")} <a href="/register">{t("createAccount")}</a>
      </p>
    </form>
  );
}
```

### Corresponding Locale File

```json
// messages/en.json
{
  "auth": {
    "login": {
      "title": "Sign In",
      "subtitle": "Welcome back! Please enter your credentials.",
      "emailLabel": "Email",
      "emailPlaceholder": "Enter your email",
      "passwordLabel": "Password",
      "passwordPlaceholder": "Enter your password",
      "signingIn": "Signing in...",
      "signInButton": "Sign In",
      "noAccount": "Don't have an account?",
      "createAccount": "Create one"
    }
  }
}
```

### add_translations Call

```json
{
  "project_root_path": "/path/to/project",
  "translations": [
    {
      "locale": "en",
      "keys": {
        "auth.login.title": "Sign In",
        "auth.login.subtitle": "Welcome back! Please enter your credentials.",
        "auth.login.emailLabel": "Email",
        "auth.login.emailPlaceholder": "Enter your email",
        "auth.login.passwordLabel": "Password",
        "auth.login.passwordPlaceholder": "Enter your password",
        "auth.login.signingIn": "Signing in...",
        "auth.login.signInButton": "Sign In",
        "auth.login.noAccount": "Don't have an account?",
        "auth.login.createAccount": "Create one"
      }
    }
  ]
}
```

## Dynamic Content Examples

### With Variables

**Before:**

```tsx
<p>
  Hello, {user.name}! You have {count} new messages.
</p>
```

**After:**

```tsx
<p>{t("greeting", { name: user.name, count })}</p>
```

**Locale file:**

```json
{
  "greeting": "Hello, {name}! You have {count} new messages."
}
```

### With Plurals

**Before:**

```tsx
<span>
  {items.length === 0 && "No items"}
  {items.length === 1 && "1 item"}
  {items.length > 1 && `${items.length} items`}
</span>
```

**After:**

```tsx
<span>{t("itemCount", { count: items.length })}</span>
```

**Locale file:**

```json
{
  "itemCount": "{count, plural, =0 {No items} =1 {1 item} other {# items}}"
}
```

### With Rich Text (HTML-like)

**Before:**

```tsx
<p>
  By continuing, you agree to our <a href="/terms">Terms of Service</a> and{" "}
  <a href="/privacy">Privacy Policy</a>.
</p>
```

**After:**

```tsx
<p>
  {t.rich("legal", {
    terms: (chunks) => <a href="/terms">{chunks}</a>,
    privacy: (chunks) => <a href="/privacy">{chunks}</a>,
  })}
</p>
```

**Locale file:**

```json
{
  "legal": "By continuing, you agree to our <terms>Terms of Service</terms> and <privacy>Privacy Policy</privacy>."
}
```

## Handling Different Contexts

### Server Components

```tsx
// app/[locale]/page.tsx
import { getTranslations } from "next-intl/server";

export default async function HomePage() {
  const t = await getTranslations("homePage");

  return (
    <main>
      <h1>{t("title")}</h1>
    </main>
  );
}
```

### Client Components

```tsx
"use client";

import { useTranslations } from "next-intl";

export function ClientComponent() {
  const t = useTranslations("component");

  return <div>{t("message")}</div>;
}
```

### With Namespace vs Without

**With namespace (recommended for components with many keys):**

```tsx
const t = useTranslations("auth.login");
// Use: t("title"), t("submit")
// Keys: auth.login.title, auth.login.submit
```

**Without namespace (for scattered keys):**

```tsx
const t = useTranslations();
// Use: t("auth.login.title"), t("common.submit")
// Keys: auth.login.title, common.submit
```

## Multi-Locale add_translations Example

When syncing translations to multiple locales:

```json
{
  "project_root_path": "/path/to/project",
  "translations": [
    {
      "locale": "en",
      "keys": {
        "common.submit": "Submit",
        "common.cancel": "Cancel",
        "common.loading": "Loading..."
      }
    },
    {
      "locale": "de",
      "keys": {
        "common.submit": "Absenden",
        "common.cancel": "Abbrechen",
        "common.loading": "Wird geladen..."
      }
    },
    {
      "locale": "fr",
      "keys": {
        "common.submit": "Soumettre",
        "common.cancel": "Annuler",
        "common.loading": "Chargement..."
      }
    },
    {
      "locale": "ja",
      "keys": {
        "common.submit": "送信",
        "common.cancel": "キャンセル",
        "common.loading": "読み込み中..."
      }
    }
  ]
}
```

## Attributes That Need Translation

glot checks these attributes for hardcoded text:

| Attribute              | Example                                    |
| ---------------------- | ------------------------------------------ |
| `placeholder`          | `<input placeholder={t("search")} />`      |
| `title`                | `<button title={t("tooltip")} />`          |
| `alt`                  | `<img alt={t("imageDesc")} />`             |
| `aria-label`           | `<button aria-label={t("close")} />`       |
| `aria-description`     | `<div aria-description={t("desc")} />`     |
| `aria-placeholder`     | `<input aria-placeholder={t("hint")} />`   |
| `aria-roledescription` | `<div aria-roledescription={t("role")} />` |
| `aria-valuetext`       | `<input aria-valuetext={t("value")} />`    |

## Ignoring Specific Text

If some hardcoded text should NOT be translated (e.g., brand names, code):

**Option 1: glot-disable-next-line comment**

```tsx
{
  /* glot-disable-next-line */
}
<span>ACME Corp</span>;
```

**Option 2: Add to ignore_texts in .glotrc.json**

```json
{
  "ignore_texts": ["ACME Corp", "GitHub", "TypeScript"]
}
```

## Fixing Untranslated Values

When AI translation tools copy text without translating, glot can detect these issues.

### scan_untranslated Response

```json
{
  "totalCount": 3,
  "items": [
    {
      "key": "common.submit",
      "value": "Submit",
      "locale": "de",
      "primaryLocale": "en"
    },
    {
      "key": "common.cancel",
      "value": "Cancel",
      "locale": "de",
      "primaryLocale": "en"
    },
    {
      "key": "auth.login.title",
      "value": "Sign In",
      "locale": "fr",
      "primaryLocale": "en"
    }
  ],
  "pagination": {
    "offset": 0,
    "limit": 50,
    "hasMore": false
  }
}
```

### Fixing with Correct Translations

```json
{
  "project_root_path": "/path/to/project",
  "translations": [
    {
      "locale": "de",
      "keys": {
        "common.submit": "Absenden",
        "common.cancel": "Abbrechen"
      }
    },
    {
      "locale": "fr",
      "keys": {
        "auth.login.title": "Connexion"
      }
    }
  ]
}
```

### Values That Should Stay the Same

Some values are intentionally identical across locales:

- Brand names: "GitHub", "TypeScript", "Next.js"
- Technical terms that aren't translated in the target language
- Proper nouns

For these, you can either:

1. Ignore them (they will continue to show as warnings)
2. Add them to `ignore_texts` in `.glotrc.json` if they appear frequently
