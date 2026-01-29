import { useTranslations } from 'next-intl';

export function Legacy() {
  const t = useTranslations('legacy');

  return (
    <div>
      {/* Suppressed for gradual migration */}
      {/* glot-disable-next-line hardcoded */}
      <h1>Legacy Header</h1>

      {/* glot-disable-next-line hardcoded */}
      <p>This component is being gradually migrated</p>

      {/* Correct usage */}
      <button>{t('continue')}</button>

      {/* Multiple suppressed lines */}
      {/* glot-disable */}
      <div>
        <span>Old Component</span>
        <span>Will be refactored soon</span>
      </div>
      {/* glot-enable */}

      {/* Back to normal checking */}
      <p>{t('footer')}</p>
    </div>
  );
}
