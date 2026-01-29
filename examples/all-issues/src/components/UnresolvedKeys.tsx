import { useTranslations } from 'next-intl';

export function UnresolvedKeys() {
  const t = useTranslations();
  const dynamicKey = Math.random() > 0.5 ? 'key1' : 'key2';
  const userRole = 'admin';

  return (
    <div>
      {/* Dynamic keys that can't be resolved statically */}
      <p>{t(`dynamic.${dynamicKey}`)}</p>
      <p>{t(`roles.${userRole}.name`)}</p>
      <p>{t(getDynamicKey())}</p>
    </div>
  );
}

function getDynamicKey() {
  return 'some.key';
}
