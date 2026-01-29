import { useTranslations } from 'next-intl';

export function Dynamic() {
  const t = useTranslations('dynamic');
  const userRole = 'admin'; // Could be: admin, editor, viewer

  return (
    <div>
      {/* Dynamic key with annotation */}
      {/* glot-message-keys "dynamic.roles.admin.name" "dynamic.roles.editor.name" "dynamic.roles.viewer.name" */}
      <p>{t(`roles.${userRole}.name`)}</p>

      {/* Another dynamic pattern */}
      {/* glot-message-keys "dynamic.actions.create" "dynamic.actions.read" "dynamic.actions.update" "dynamic.actions.delete" */}
      <span>{t(`actions.${getAction()}`)}</span>
    </div>
  );
}

function getAction() {
  return 'create';
}
