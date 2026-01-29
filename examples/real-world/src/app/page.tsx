import { useTranslations } from 'next-intl';
import { Header } from '../components/Header';
import { Legacy } from '../components/Legacy';
import { Dynamic } from '../components/Dynamic';

export default function Page() {
  const t = useTranslations('home');

  return (
    <main>
      <Header />
      <h2>{t('welcome')}</h2>
      <Legacy />
      <Dynamic />
    </main>
  );
}
