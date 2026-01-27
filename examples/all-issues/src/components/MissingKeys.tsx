import { useTranslations } from 'next-intl';

export function MissingKeys() {
  const t = useTranslations();

  return (
    <div>
      {/* These keys don't exist in messages/en.json */}
      <h1>{t('nonexistent.title')}</h1>
      <p>{t('missing.description')}</p>
      <button>{t('actions.submit')}</button>
    </div>
  );
}
