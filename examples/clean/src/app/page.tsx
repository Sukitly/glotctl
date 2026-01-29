import { useTranslations } from 'next-intl';
import { Button } from '../components/Button';
import { Form } from '../components/Form';

export default function Page() {
  const t = useTranslations('home');

  return (
    <main>
      <header>
        <h1>{t('welcome')}</h1>
        <p>{t('tagline')}</p>
      </header>
      <Button />
      <Form />
    </main>
  );
}
