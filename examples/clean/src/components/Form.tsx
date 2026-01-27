import { useTranslations } from 'next-intl';

export function Form() {
  const t = useTranslations('form');
  const isActive = true;

  return (
    <form>
      <h1>{t('title')}</h1>
      <p>{t('description')}</p>

      <input
        type="email"
        placeholder={t('email.placeholder')}
        aria-label={t('email.label')}
      />

      <input
        type="password"
        placeholder={t('password.placeholder')}
        aria-label={t('password.label')}
      />

      <p>{isActive ? t('status.active') : t('status.inactive')}</p>

      <button type="submit">{t('submit')}</button>
    </form>
  );
}
