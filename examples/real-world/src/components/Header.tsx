import { useTranslations } from 'next-intl';

export function Header() {
  const t = useTranslations('header');
  const isLoggedIn = true;

  return (
    <header>
      {/* Correct usage */}
      <h1>{t('title')}</h1>

      {/* Some hardcoded text */}
      <button>Sign Out</button>

      {/* Correct conditional */}
      <span>{isLoggedIn ? t('status.logged_in') : t('status.logged_out')}</span>

      {/* Hardcoded in attribute */}
      <input placeholder="Search..." />
    </header>
  );
}
