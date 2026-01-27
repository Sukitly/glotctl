import { useTranslations } from 'next-intl';

export function ParseError() {
  const t = useTranslations();

  return (
    <div>
      <h1>{t('valid.key')}</h1>
      {/* Invalid JSX syntax below - missing closing tag */}
      <button>Click Me
      <p>This will cause a parse error</p>
    </div>
  );
}
