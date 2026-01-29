import { useTranslations } from 'next-intl';

export function Button() {
  const t = useTranslations('common');

  return (
    <button
      title={t('button.tooltip')}
      aria-label={t('button.aria_label')}
    >
      {t('button.submit')}
    </button>
  );
}
