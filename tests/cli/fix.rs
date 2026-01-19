use anyhow::Result;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

fn setup_config(test: &CliTest) -> Result<()> {
    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./messages",
            "primaryLocale": "en"
        }"#,
    )
}

#[test]
fn test_fix_dry_run() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Line starts with "<" so should use {/* */} comment
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Button({ prefix }: { prefix: string }) {
    return (
        <button>{t(`${prefix}.submit`)}</button>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"submit": "Submit"}}}"#,
    )?;

    assert_cmd_snapshot!(test.fix_command());
    Ok(())
}

#[test]
fn test_fix_apply_jsx_context() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Line starts with "<" so should use {/* */} comment
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Button({ prefix }: { prefix: string }) {
    return (
        <button>{t(`${prefix}.submit`)}</button>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"submit": "Submit"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify JSX comment was inserted (line starts with <)
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("{/* glot-message-keys \"Common.*.submit\" */}"),
        "Expected JSX comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_fix_apply_non_jsx_context() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // t() call in a callback, not in JSX - should use // comment style
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Logger({ code }: { code: string }) {
    console.log(t(`error.${code}`));
    return null;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"error": {"unknown": "Unknown"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify JS comment was inserted
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("// glot-message-keys \"Common.error.*\""),
        "Expected JS comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_fix_no_dynamic_keys() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Only static keys, no dynamic keys
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

export function App() {
    const t = useTranslations("Common");
    return <div>{t("hello")}</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"hello": "Hello"}}"#)?;

    assert_cmd_snapshot!(test.fix_command());
    Ok(())
}

#[test]
fn test_fix_variable_key_unfixable() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Pure variable key - cannot infer pattern
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Dynamic({ keyName }: { keyName: string }) {
    return <span>{t(keyName)}</span>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"hello": "Hello"}}"#)?;

    assert_cmd_snapshot!(test.fix_command());
    Ok(())
}

#[test]
fn test_fix_mixed_fixable_unfixable() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Mix of template (fixable) and variable (unfixable) keys
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Mixed({ prefix, keyName }: { prefix: string; keyName: string }) {
    return (
        <div>
            <span>{t(`${prefix}.title`)}</span>
            <span>{t(keyName)}</span>
        </div>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"title": "Title"}}}"#,
    )?;

    assert_cmd_snapshot!(test.fix_command());
    Ok(())
}

#[test]
fn test_fix_multiple_same_file() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Multiple dynamic keys in same file
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Multi({ prefix, code }: { prefix: string; code: string }) {
    return (
        <div>
            <span>{t(`${prefix}.title`)}</span>
            <span>{t(`${prefix}.description`)}</span>
            <span>{t(`error.${code}`)}</span>
        </div>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"title": "Title", "description": "Desc"}, "error": {"unknown": "Unknown"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify all comments were inserted
    let content = test.read_file("src/app.tsx")?;
    let comment_count = content.matches("glot-message-keys").count();
    assert_eq!(
        comment_count, 3,
        "Expected 3 comments, got {}:\n{}",
        comment_count, content
    );
    Ok(())
}

#[test]
fn test_fix_multiple_files() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Dynamic keys in multiple files
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function App({ prefix }: { prefix: string }) {
    return <div>{t(`${prefix}.title`)}</div>;
}
"#,
    )?;

    test.write_file(
        "src/button.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Button");

export function Button({ type }: { type: string }) {
    return <button>{t(`${type}.label`)}</button>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"title": "Title"}}, "Button": {"submit": {"label": "Submit"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify both files were modified
    let app_content = test.read_file("src/app.tsx")?;
    assert!(
        app_content.contains("glot-message-keys"),
        "Expected comment in app.tsx:\n{}",
        app_content
    );

    let button_content = test.read_file("src/button.tsx")?;
    assert!(
        button_content.contains("glot-message-keys"),
        "Expected comment in button.tsx:\n{}",
        button_content
    );
    Ok(())
}

#[test]
fn test_fix_preserves_indentation() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function App({ prefix }: { prefix: string }) {
    return (
        <div>
            <span>
                {t(`${prefix}.deeply.nested`)}
            </span>
        </div>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"deeply": {"nested": "Nested"}}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Comment should have matching indentation (16 spaces for the t() call line)
    // Line starts with "{" and is in JSX context, so uses {/* */} comment
    assert!(
        content.contains("                {/* glot-message-keys"),
        "Expected JSX comment with matching indentation, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_fix_multiple_same_line() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Two dynamic keys on the same line
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Multi({ a, b }: { a: string; b: string }) {
    return (
        <div>
            <span>{t(`${a}.x`)} {t(`${b}.y`)}</span>
        </div>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"x": "X", "y": "Y"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify single comment with both patterns was inserted
    let content = test.read_file("src/app.tsx")?;
    let comment_count = content.matches("glot-message-keys").count();
    assert_eq!(
        comment_count, 1,
        "Expected 1 merged comment, got {}:\n{}",
        comment_count, content
    );
    // Should contain both patterns
    assert!(
        content.contains("\"Common.*.x\"") && content.contains("\"Common.*.y\""),
        "Expected both patterns in comment:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_fix_single_line_jsx_return() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Single line return with JSX - line starts with "return", not "<"
    // So should use // comment, not {/* */}
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function Button({ prefix }: { prefix: string }) {
    return <button>{t(`${prefix}.label`)}</button>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"label": "Label"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify JS comment was used (not JSX comment)
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("// glot-message-keys"),
        "Expected JS comment for single-line return, got:\n{}",
        content
    );
    assert!(
        !content.contains("{/* glot-message-keys"),
        "Should NOT have JSX comment for single-line return, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_fix_jsx_attribute() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Dynamic key in JSX attribute (not JSX children)
    // The comment is inserted ABOVE the line, which is JS context
    // So it should use // comment, not {/* */}
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";

const t = useTranslations("Common");

export function App({ prefix }: { prefix: string }) {
    return <Button label={t(`${prefix}.label`)} />;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"test": {"label": "Label"}}}"#,
    )?;

    let mut cmd = test.fix_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify JS comment was used (comment goes before the JSX element, which is JS context)
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("// glot-message-keys"),
        "Expected JS comment for JSX attribute, got:\n{}",
        content
    );
    assert!(
        !content.contains("{/* glot-message-keys"),
        "Should NOT have JSX comment for JSX attribute, got:\n{}",
        content
    );
    Ok(())
}
