use anyhow::Result;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn test_hardcoded_text() -> Result<()> {
    let test = CliTest::with_file(
        "src/app/[locale]/app.tsx",
        r#"
  export function Button() {
      return <button>Submit</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_clean_file() -> Result<()> {
    let test = CliTest::with_file(
        "app/app.tsx",
        r#"
  export function Button() {
      return <button>{t('submit')}</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_multiple_issues() -> Result<()> {
    let test = CliTest::with_file(
        "src/app/[locale]/app.tsx",
        r#"
  export function Card() {
      return (
          <div>
              <h1>Welcome</h1>
              <p placeholder="Enter name">Hello World</p>
          </div>
      );
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_help() -> Result<()> {
    let test = CliTest::new()?;

    assert_cmd_snapshot!(test.command().arg("--help"));

    Ok(())
}

#[test]
fn test_config_ignores() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
         "ignores": ["**/generated/**"],
         "includes": []
     }"#,
    )?;

    test.write_file("src/app.tsx", r#"<div>Hello</div>"#)?;

    test.write_file("generated/types.tsx", r#"<div>Ignored</div>"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_config_includes() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
         "includes": ["src"]
     }"#,
    )?;

    test.write_file("src/app.tsx", r#"<div>Hello</div>"#)?;
    test.write_file("lib/utils.tsx", r#"<div>Not scanned</div>"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_config_checked_attributes() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
         "includes": [],
         "checkedAttributes": ["placeholder"]
     }"#,
    )?;

    test.write_file(
        "src/app.tsx",
        r#"<input placeholder="Name" title="Title" />"#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_no_config_uses_defaults() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file("src/app/[locale]/app.tsx", r#"<div>In src</div>"#)?;
    test.write_file("src/lib/utils.tsx", r#"<div>In lib</div>"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_missing_key() -> Result<()> {
    let test = CliTest::new()?;

    // Create config
    test.write_file(
        ".glotrc.json",
        r#"{
              "includes": ["src"],
              "messagesDir": "./messages",
              "primaryLocale": "en"
          }"#,
    )?;

    // Create message file (missing "Common.submit")
    test.write_file(
        "messages/en.json",
        r#"{
              "Common": {
                  "cancel": "Cancel"
              }
          }"#,
    )?;

    // Create TSX file using a missing key
    test.write_file(
        "src/app.tsx",
        r#"
  const t = useTranslations("Common");
  export function Button() {
      return <button>{t("submit")}</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_no_missing_key_when_defined() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
              "includes": ["src"],
              "messagesDir": "./messages",
              "primaryLocale": "en"
          }"#,
    )?;

    // Key is defined
    test.write_file(
        "messages/en.json",
        r#"{
              "Common": {
                  "submit": "Submit"
              }
          }"#,
    )?;

    test.write_file(
        "src/app.tsx",
        r#"
  const t = useTranslations("Common");
  export function Button() {
      return <button>{t("submit")}</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_missing_key_with_hardcoded_text() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
              "includes": ["src"],
              "messagesDir": "./messages",
              "primaryLocale": "en"
          }"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {}}"#)?;

    test.write_file(
        "src/app.tsx",
        r#"
  const t = useTranslations("Common");
  export function Form() {
      return (
          <form>
              <button>{t("submit")}</button>
              <span>Hardcoded</span>
          </form>
      );
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_dynamic_key_warning() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
              "includes": ["src"],
              "messagesDir": "./messages",
              "primaryLocale": "en"
          }"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;

    test.write_file(
        "src/app.tsx",
        r#"
  const t = useTranslations("Common");
  export function Button({ keyName }) {
      return <button>{t(keyName)}</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_template_with_expr_warning() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
              "includes": ["src"],
              "messagesDir": "./messages",
              "primaryLocale": "en"
          }"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;

    test.write_file(
        "src/app.tsx",
        r#"
  const t = useTranslations("Common");
  export function Button({ prefix }) {
      return <button>{t(`${prefix}.submit`)}</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_template_with_expr_non_jsx_context() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"error": {"unknown": "Unknown error"}}}"#,
    )?;

    // t() call in a callback, not in JSX - should use // comment style
    test.write_file(
        "src/app.tsx",
        r#"
import {useTranslations} from 'next-intl';
export function Component() {
    const t = useTranslations('Common');
    const handleError = (code: string) => {
        console.log(t(`error.${code}`));
    };
    return <button onClick={() => handleError('unknown')}>Click</button>;
}
"#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_replica_lag() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Primary locale has both keys
    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "submit": "Submit",
                "cancel": "Cancel"
            }
        }"#,
    )?;

    // zh is missing "cancel"
    test.write_file(
        "messages/zh.json",
        r#"{
            "Common": {
                "submit": "提交"
            }
        }"#,
    )?;

    // Empty source file (no hardcoded text)
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function Button() {
    return <button>{t("submit")}</button>;
}
"#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_replica_lag_multiple_locales() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "submit": "Submit"
            }
        }"#,
    )?;

    // Both zh and ja are missing the key
    test.write_file("messages/zh.json", r#"{}"#)?;
    test.write_file("messages/ja.json", r#"{}"#)?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_replica_lag_none() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;

    test.write_file("messages/zh.json", r#"{"Common": {"submit": "提交"}}"#)?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_unused_key() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Define a key that's never used
    test.write_file(
        "messages/en.json",
        r#"{"Common": {"submit": "Submit", "unused": "Unused Button"}}"#,
    )?;

    // Only use "submit", not "unused"
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function Button() {
    return <button>{t("submit")}</button>;
}
"#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_no_unused_key_when_used() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function Button() {
    return <button>{t("submit")}</button>;
}
"#,
    )?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_orphan_key() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Primary locale only has "submit"
    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;

    // zh has an extra key "oldKey" that doesn't exist in en
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"submit": "提交", "oldKey": "旧的按钮"}}"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_orphan_key_multiple_locales() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;

    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"submit": "提交", "orphan1": "孤儿1"}}"#,
    )?;

    test.write_file(
        "messages/ja.json",
        r#"{"Common": {"submit": "送信", "orphan2": "孤児2"}}"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

// ============================================
// Subcommand tests
// ============================================

#[test]
fn test_subcommand_hardcoded() -> Result<()> {
    let test = CliTest::with_file(
        "src/app/[locale]/app.tsx",
        r#"
  export function Button() {
      return <button>Submit</button>;
  }
  "#,
    )?;

    assert_cmd_snapshot!(test.check_command().arg("hardcoded"));

    Ok(())
}

#[test]
fn test_subcommand_missing() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;
    test.write_file("messages/en.json", r#"{"Common": {"cancel": "Cancel"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"cancel": "取消"}}"#)?;
    test.write_file(
        "src/app.tsx",
        r#"
        const t = useTranslations("Common");
        t("submit");
        "#,
    )?;

    assert_cmd_snapshot!(test.check_command().arg("missing"));

    Ok(())
}

#[test]
fn test_subcommand_orphan() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;
    test.write_file("messages/en.json", r#"{"Common": {"submit": "Submit"}}"#)?;
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"submit": "提交", "oldKey": "旧的按钮"}}"#,
    )?;
    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    assert_cmd_snapshot!(test.check_command().arg("orphan"));

    Ok(())
}

// ============================================
// Parameter combination tests
// ============================================

#[test]
fn test_subcommand_with_path_arg() -> Result<()> {
    let test = CliTest::new()?;

    // Create files in a subdirectory
    test.write_file(
        "subdir/app.tsx",
        r#"
  export function Button() {
      return <button>Submit</button>;
  }
  "#,
    )?;

    // Without --path, should find nothing (default is "." which has no tsx files at root)
    assert_cmd_snapshot!(test.check_command().arg("hardcoded"));

    // With --path subdir, should find the issue (args before subcommand)
    assert_cmd_snapshot!(
        test.check_command()
            .arg("--path")
            .arg("subdir")
            .arg("hardcoded")
    );

    Ok(())
}

#[test]
fn test_subcommand_with_verbose_arg() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./nonexistent",
            "primaryLocale": "en"
        }"#,
    )?;
    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    // Without --verbose, no warning output
    assert_cmd_snapshot!(test.check_command().arg("missing"));

    // With --verbose, should show warning about missing messages dir (args before subcommand)
    assert_cmd_snapshot!(test.check_command().arg("--verbose").arg("missing"));

    Ok(())
}

// ============================================
// Schema Factory Function Tests
// ============================================

#[test]
fn test_schema_factory_keys_tracked_as_used() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Define all keys including those used in schema
    test.write_file(
        "messages/en.json",
        r#"{
            "Form": {
                "titleRequired": "Title is required",
                "descMax": "Description too long"
            }
        }"#,
    )?;

    // Schema factory function file
    test.write_file(
        "src/schemas.ts",
        r#"
export const createFormSchema = (t) => z.object({
    title: z.string().min(1, t("titleRequired")),
    desc: z.string().max(100, t("descMax")),
});
"#,
    )?;

    // Component that uses the schema
    test.write_file(
        "src/form.tsx",
        r#"
const tForm = useTranslations("Form");
const schema = createFormSchema(tForm);
"#,
    )?;

    // Keys should be tracked as used, so no unused key warnings
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_schema_factory_missing_key_reported() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Missing "titleRequired" key
    test.write_file(
        "messages/en.json",
        r#"{
            "Form": {
                "descMax": "Description too long"
            }
        }"#,
    )?;

    test.write_file(
        "src/schemas.ts",
        r#"
export const createFormSchema = (t) => z.object({
    title: z.string().min(1, t("titleRequired")),
    desc: z.string().max(100, t("descMax")),
});
"#,
    )?;

    test.write_file(
        "src/form.tsx",
        r#"
const tForm = useTranslations("Form");
const schema = createFormSchema(tForm);
"#,
    )?;

    // Should report missing key
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_schema_factory_nested_calls() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All keys defined
    test.write_file(
        "messages/en.json",
        r#"{
            "Form": {
                "baseKey": "Base field required",
                "extendedKey": "Extended field required"
            }
        }"#,
    )?;

    // Nested schema functions
    test.write_file(
        "src/schemas.ts",
        r#"
export const createBaseSchema = (t) => z.object({
    base: z.string().min(1, t("baseKey")),
});

export const createExtendedSchema = (t) =>
    createBaseSchema(t).extend({
        extended: z.string().min(1, t("extendedKey")),
    });
"#,
    )?;

    test.write_file(
        "src/form.tsx",
        r#"
const tForm = useTranslations("Form");
const schema = createExtendedSchema(tForm);
"#,
    )?;

    // Both base and extended keys should be tracked
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_dynamic_key_all_candidates_exist() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All candidate keys exist in messages
    test.write_file(
        "messages/en.json",
        r#"{
            "Tools": {
                "createNovel": "Create Novel",
                "updateNovel": "Update Novel"
            }
        }"#,
    )?;

    test.write_file(
        "src/app.tsx",
        r#"
const toolKeys = {
    createNovel: "createNovel",
    updateNovel: "updateNovel",
};
const t = useTranslations("Tools");
export function Tool({ toolName }) {
    const key = toolKeys[toolName];
    return t(key);
}
"#,
    )?;

    // Should pass silently - all candidates exist
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_dynamic_key_some_candidates_missing() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Only one key exists, other is missing
    test.write_file(
        "messages/en.json",
        r#"{
            "Tools": {
                "createNovel": "Create Novel"
            }
        }"#,
    )?;

    test.write_file(
        "src/app.tsx",
        r#"
const toolKeys = {
    createNovel: "createNovel",
    updateNovel: "updateNovel",
};
const t = useTranslations("Tools");
export function Tool({ toolName }) {
    const key = toolKeys[toolName];
    return t(key);
}
"#,
    )?;

    // Should report error for missing candidate
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

// ============================================
// String Array Iteration Tests
// ============================================

#[test]
fn test_string_array_iteration_same_file_all_exist() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All candidate keys exist
    test.write_file(
        "messages/en.json",
        r#"{
            "Features": {
                "prefix.save": "Save",
                "prefix.load": "Load"
            }
        }"#,
    )?;

    // String array and component in SAME file
    test.write_file(
        "src/app.tsx",
        r#"
const FEATURE_KEYS = ["save", "load"] as const;
const t = useTranslations("Features");
FEATURE_KEYS.map((k) => t(`prefix.${k}`));
"#,
    )?;

    // Expected: No errors - all keys exist
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_string_array_iteration_same_file_some_missing() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Only one key exists
    test.write_file(
        "messages/en.json",
        r#"{
            "Features": {
                "prefix.save": "Save"
            }
        }"#,
    )?;

    // String array and component in SAME file
    test.write_file(
        "src/app.tsx",
        r#"
const FEATURE_KEYS = ["save", "load"] as const;
const t = useTranslations("Features");
FEATURE_KEYS.map((k) => t(`prefix.${k}`));
"#,
    )?;

    // Expected: Error for missing "Features.prefix.load"
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

// ============================================
// Translation Props Tests
// ============================================

#[test]
fn test_translation_prop_keys_tracked_as_used() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All keys defined
    test.write_file(
        "messages/en.json",
        r#"{
            "Landing": {
                "title": "Welcome",
                "description": "This is a description"
            }
        }"#,
    )?;

    // Parent component passes t to child
    test.write_file(
        "src/page.tsx",
        r#"
const t = useTranslations("Landing");
export function Page() {
    return <LandingContent t={t} />;
}
"#,
    )?;

    // Child component receives t as prop and uses it
    test.write_file(
        "src/landing-content.tsx",
        r#"
export function LandingContent({ t }: Props) {
    return (
        <div>
            <h1>{t("title")}</h1>
            <p>{t("description")}</p>
        </div>
    );
}
"#,
    )?;

    // Keys should be tracked as used via the prop, no unused warnings
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_translation_prop_missing_key_reported() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Missing "subtitle" key
    test.write_file(
        "messages/en.json",
        r#"{
            "Landing": {
                "title": "Welcome"
            }
        }"#,
    )?;

    // Parent component passes t to child
    test.write_file(
        "src/page.tsx",
        r#"
const t = useTranslations("Landing");
export function Page() {
    return <LandingContent t={t} />;
}
"#,
    )?;

    // Child component uses a key that doesn't exist
    test.write_file(
        "src/landing-content.tsx",
        r#"
export function LandingContent({ t }: Props) {
    return (
        <div>
            <h1>{t("title")}</h1>
            <p>{t("subtitle")}</p>
        </div>
    );
}
"#,
    )?;

    // Should report missing key "subtitle"
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_translation_prop_arrow_function_component() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
            "Card": {
                "title": "Card Title"
            }
        }"#,
    )?;

    // Parent passes t to arrow function child
    test.write_file(
        "src/page.tsx",
        r#"
const t = useTranslations("Card");
export function Page() {
    return <CardComponent t={t} />;
}
"#,
    )?;

    // Arrow function component
    test.write_file(
        "src/card.tsx",
        r#"
export const CardComponent = ({ t }: Props) => {
    return <div>{t("title")}</div>;
};
"#,
    )?;

    // Should pass - key is tracked via prop
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_translation_prop_with_relative_glot_message_keys() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All dynamic keys exist
    test.write_file(
        "messages/en.json",
        r#"{
            "Features": {
                "items": {
                    "save": { "title": "Save" },
                    "load": { "title": "Load" }
                }
            }
        }"#,
    )?;

    // Parent passes t to child
    test.write_file(
        "src/page.tsx",
        r#"
const t = useTranslations("Features");
export function Page() {
    return <FeatureList t={t} items={items} />;
}
"#,
    )?;

    // Child uses dynamic keys with relative glot-message-keys annotation
    test.write_file(
        "src/feature-list.tsx",
        r#"
export function FeatureList({ t, items }: Props) {
    return items.map(item => {
        // glot-message-keys ".items.*.title"
        return <span>{t(`items.${item.key}.title`)}</span>;
    });
}
"#,
    )?;

    // Should pass - relative pattern expanded with namespace from prop
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

// ============================================
// Translation Function Call Tests
// ============================================

#[test]
fn test_translation_fn_call_same_file_used_key() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All keys defined
    test.write_file(
        "messages/en.json",
        r#"{
            "Usage": {
                "monthly": "Monthly",
                "yearly": "Yearly"
            }
        }"#,
    )?;

    // Utility function and component in SAME file
    // usageLabels receives t as argument and uses it to translate keys
    test.write_file(
        "src/app.tsx",
        r#"
const usageLabels = (t) => ({
    monthly: t("monthly"),
    yearly: t("yearly"),
});

export function UsagePage() {
    const t = useTranslations("Usage");
    const labels = usageLabels(t);
    return <div>{labels.monthly}</div>;
}
"#,
    )?;

    // Keys should be tracked as used via function call, no unused warnings
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_translation_fn_call_import_alias_used_key() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All keys defined
    test.write_file(
        "messages/en.json",
        r#"{
            "Status": {
                "active": "Active",
                "inactive": "Inactive"
            }
        }"#,
    )?;

    // Utility file with named export
    test.write_file(
        "src/utils.ts",
        r#"
export const buildStatusLabels = (t) => ({
    active: t("active"),
    inactive: t("inactive"),
});
"#,
    )?;

    // Component imports with alias and calls the function
    test.write_file(
        "src/app.tsx",
        r#"
import { buildStatusLabels as statusLabels } from './utils';

export function StatusPage() {
    const t = useTranslations("Status");
    const labels = statusLabels(t);
    return <div>{labels.active}</div>;
}
"#,
    )?;

    // Keys should be tracked via aliased import, no unused warnings
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_translation_fn_call_default_export_used_key() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // All keys defined
    test.write_file(
        "messages/en.json",
        r#"{
            "Plan": {
                "free": "Free Plan",
                "pro": "Pro Plan"
            }
        }"#,
    )?;

    // Utility file with default export
    test.write_file(
        "src/plan-labels.ts",
        r#"
export default function buildPlanLabels(t) {
    return {
        free: t("free"),
        pro: t("pro"),
    };
}
"#,
    )?;

    // Component uses default import
    test.write_file(
        "src/app.tsx",
        r#"
import buildPlanLabels from './plan-labels';

export function PlanPage() {
    const t = useTranslations("Plan");
    const labels = buildPlanLabels(t);
    return <div>{labels.free}</div>;
}
"#,
    )?;

    // Keys should be tracked via default import, no unused warnings
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

#[test]
fn test_translation_fn_call_param_shadowing() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Only "outer" key is defined, "inner" is NOT defined
    test.write_file(
        "messages/en.json",
        r#"{
            "Outer": {
                "outerKey": "Outer Value"
            }
        }"#,
    )?;

    // Outer function receives t from registry, inner function shadows t
    // Inner's t("innerKey") should NOT be tracked (parameter shadows outer binding)
    test.write_file(
        "src/utils.ts",
        r#"
export const outerFunc = (t) => {
    const innerFunc = (t) => {
        return t("innerKey");  // Should NOT be tracked - t is shadowed
    };
    return t("outerKey");  // Should be tracked
};
"#,
    )?;

    // Component calls outerFunc with translation function
    test.write_file(
        "src/app.tsx",
        r#"
import { outerFunc } from './utils';

export function Page() {
    const t = useTranslations("Outer");
    const result = outerFunc(t);
    return <div>{result}</div>;
}
"#,
    )?;

    // Only "outerKey" should be tracked, "innerKey" should be ignored (shadowed)
    // No missing key error for "innerKey" since it's not tracked
    assert_cmd_snapshot!(test.check_command());

    Ok(())
}

// ============================================
// Untranslated Value Tests
// ============================================

#[test]
fn test_untranslated_value() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // English (primary locale)
    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "submit": "Submit",
                "cancel": "Cancel"
            }
        }"#,
    )?;

    // Chinese locale has same value as English (not translated)
    test.write_file(
        "messages/zh.json",
        r#"{
            "Common": {
                "submit": "Submit",
                "cancel": "Cancel"
            }
        }"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    // Should detect untranslated values
    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_untranslated_correct_translation() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // English (primary locale)
    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "welcome": "Welcome to our platform"
            }
        }"#,
    )?;

    // Correctly translated to Chinese
    test.write_file(
        "messages/zh.json",
        r#"{
            "Common": {
                "welcome": "欢迎来到我们的平台"
            }
        }"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    // Should pass without issues
    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_untranslated_short_text() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "ok": "OK",
                "no": "No"
            }
        }"#,
    )?;

    // Short texts that are the same - should be caught by untranslated check
    test.write_file(
        "messages/zh.json",
        r#"{
            "Common": {
                "ok": "OK",
                "no": "No"
            }
        }"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_untranslated_any_locale() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "greeting": "Hello, welcome to our platform"
            }
        }"#,
    )?;

    // Thai locale - value is same as English, should be flagged as untranslated
    test.write_file(
        "messages/th.json",
        r#"{
            "Common": {
                "greeting": "Hello, welcome to our platform"
            }
        }"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    // Should detect untranslated
    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_untranslated_skip_primary_locale() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    // Only primary locale exists
    test.write_file(
        "messages/en.json",
        r#"{
            "Common": {
                "submit": "Submit"
            }
        }"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    // Should pass - primary locale is skipped
    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_subcommand_untranslated() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"submit": "Submit button"}}"#,
    )?;

    // Untranslated
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"submit": "Submit button"}}"#,
    )?;

    test.write_file("src/app.tsx", r#"const x = 1;"#)?;

    // Should only run untranslated check
    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_untranslated_with_usages() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"submit": "Submit", "cancel": "Cancel"}}"#,
    )?;

    // Untranslated - same value as English
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"submit": "Submit", "cancel": "Cancel"}}"#,
    )?;

    // Use the keys in code
    test.write_file(
        "src/app.tsx",
        r#"
import {useTranslations} from 'next-intl';

export default function App() {
    const t = useTranslations('Common');
    return <button>{t('submit')}</button>;
}
"#,
    )?;

    // Should show usage location
    assert_cmd_snapshot!(test.check_command().arg("untranslated"));

    Ok(())
}

#[test]
fn test_replica_lag_with_usages() -> Result<()> {
    let test = CliTest::new()?;

    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"submit": "Submit", "cancel": "Cancel"}}"#,
    )?;

    // Missing cancel in Chinese
    test.write_file("messages/zh.json", r#"{"Common": {"submit": "提交"}}"#)?;

    // Use both keys in code
    test.write_file(
        "src/app.tsx",
        r#"
import {useTranslations} from 'next-intl';

export default function App() {
    const t = useTranslations('Common');
    return (
        <div>
            <button>{t('submit')}</button>
            <button>{t('cancel')}</button>
        </div>
    );
}
"#,
    )?;

    // Should show usage location for replica-lag
    assert_cmd_snapshot!(test.check_command().arg("missing"));

    Ok(())
}
