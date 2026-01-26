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
fn test_baseline_dry_run() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return <div>Hello World</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    assert_cmd_snapshot!(test.baseline_command());
    Ok(())
}

#[test]
fn test_baseline_apply_jsx_context() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return <div>Hello World</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify JSX comment was inserted
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_apply_js_context() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return <input placeholder="Enter name" />;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Attributes are in JS context (not JSX children)
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("// glot-disable-next-line hardcoded"),
        "Expected JS comment for attribute, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_no_hardcoded_text() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

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

    assert_cmd_snapshot!(test.baseline_command());
    Ok(())
}

#[test]
fn test_baseline_multiple_same_line() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Multiple hardcoded texts on same line should get single comment
    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return <div>Hello World</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    cmd.output()?;

    let content = test.read_file("src/app.tsx")?;
    // Should only have one comment for the line
    let comment_count = content.matches("glot-disable-next-line").count();
    assert_eq!(
        comment_count, 1,
        "Expected 1 comment, got {}:\n{}",
        comment_count, content
    );
    Ok(())
}

#[test]
fn test_baseline_preserves_indentation() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return (
        <div>
            <span>Deeply indented</span>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Comment should have matching indentation (12 spaces for <span>)
    assert!(
        content.contains("            {/* glot-disable-next-line hardcoded */}"),
        "Expected comment with matching indentation, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_nested_element_same_line_attr() -> Result<()> {
    // When element tag starts on the same line as attribute,
    // comment goes in JSX children position → {/* */}
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return (
        <div>
            <nav aria-label="Tabs">content</nav>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Line starts with <nav, so comment is in JSX children position
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment (line starts with <), got:\n{}",
        content
    );
    assert!(
        !content.contains("// glot-disable-next-line hardcoded"),
        "Should NOT have JS comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_multiline_attr() -> Result<()> {
    // When attribute is on a separate line (multi-line format),
    // comment goes in attribute list position → //
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return (
        <div>
            <input
                placeholder="Enter name"
            />
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Line doesn't start with <, so comment is in attribute list
    assert!(
        content.contains("// glot-disable-next-line hardcoded"),
        "Expected JS comment (multi-line attr), got:\n{}",
        content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Should NOT have JSX comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_mixed_same_line_and_multiline() -> Result<()> {
    // Mix of same-line element and multi-line attributes
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return (
        <div>
            <span>JSX Text</span>
            <input
                placeholder="Multi-line attr"
            />
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // <span> line starts with < → {/* */}
    // placeholder line doesn't start with < → //
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment for text, got:\n{}",
        content
    );
    assert!(
        content.contains("// glot-disable-next-line hardcoded"),
        "Expected JS comment for multi-line attr, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_inside_jsx_expression() -> Result<()> {
    // Text inside {expr} should use // because it's JS context
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App({ logs }) {
    return (
        <div>
            {logs.length === 0 ? (
                <p>No console output</p>
            ) : (
                <span>Has output</span>
            )}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Both are inside {expr}, so use //
    let js_comment_count = content
        .matches("// glot-disable-next-line hardcoded")
        .count();
    assert_eq!(
        js_comment_count, 2,
        "Expected 2 JS comments (inside {{expr}}), got {}:\n{}",
        js_comment_count, content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Should NOT have JSX comment inside {{expr}}, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_jsx_fragment_children() -> Result<()> {
    // Text inside Fragment <> should use {/* */}
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return (
        <>
            <span>Fragment child</span>
        </>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment in fragment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_logical_and_expression() -> Result<()> {
    // {condition && <Element>} on one line - comment goes in JSX children (above the line)
    // Since the line is inside <div> children, use {/* */}
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App({ show }) {
    return (
        <div>
            {show && <span>Conditional text</span>}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Line is inside <div> children, comment goes there too → {/* */}
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment (line inside JSX children), got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_hardcoded_ternary_jsx_expression_uses_slash_comment() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App({ isPending }) {
    return (
        <div>
            {isPending
                ? "Processing"
                : "Ready"}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    let count = content
        .matches("// glot-disable-next-line hardcoded")
        .count();
    assert_eq!(
        count, 2,
        "Expected 2 JS comments for ternary branches, got {}:\n{}",
        count, content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected no JSX comments for ternary branches, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_ternary_jsx_expression_uses_slash_comment() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App({ isPending }) {
    const t = useTranslations("Common");
    return (
        <div>
            {isPending
                ? t("processing")
                : t("ready")}
        </div>
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"processing": "Processing", "ready": "Ready"}}"#,
    )?;
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"processing": "Processing", "ready": "Ready"}}"#,
    )?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    let count = content
        .matches("// glot-disable-next-line untranslated")
        .count();
    assert_eq!(
        count, 2,
        "Expected 2 JS comments for ternary branches, got {}:\n{}",
        count, content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line untranslated */}"),
        "Expected no JSX comments for ternary branches, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_map_expression() -> Result<()> {
    // {items.map(...)} - elements inside map are in expression context
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App({ items }) {
    return (
        <div>
            {items.map((item) => (
                <span key={item.id}>Item text</span>
            ))}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Inside {items.map(...)}, use //
    assert!(
        content.contains("// glot-disable-next-line hardcoded"),
        "Expected JS comment (inside map), got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_nested_element_in_expression() -> Result<()> {
    // Text on its own line inside element that's inside expression
    // should use {/* */} because comment goes in JSX children
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App({ show }) {
    return (
        <div>
            {show && (
                <div>
                    <span>
                        Nested text
                    </span>
                </div>
            )}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Text on its own line inside <span> should use {/* */}
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment for text on own line inside element, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_in_js_file_uses_slash_comment() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.js",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    const label = t("greeting");
    return label;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.js")?;
    assert!(
        content.contains("// glot-disable-next-line untranslated"),
        "Expected JS comment in .js file, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_in_ts_file_uses_slash_comment() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.ts",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    const label = t("greeting");
    return label;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.ts")?;
    assert!(
        content.contains("// glot-disable-next-line untranslated"),
        "Expected JS comment in .ts file, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_hardcoded_in_jsx_file_uses_jsx_comment() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.jsx",
        r#"export function App() {
    return <div>Hardcoded</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.jsx")?;
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment in .jsx file, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_hardcoded_in_jsx_attr_uses_slash_comment() -> Result<()> {
    // Hardcoded attribute in .jsx file should use JS comment
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.jsx",
        r#"export function App() {
    return <input placeholder="Enter name" />;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.jsx")?;
    // Attribute on same line as <input should use // comment
    assert!(
        content.contains("// glot-disable-next-line hardcoded"),
        "Expected JS comment for attribute, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_hardcoded_multiline_jsx_attr_uses_slash_comment() -> Result<()> {
    // Hardcoded multi-line attribute in .jsx file should use JS comment
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.jsx",
        r#"export function App() {
    return (
        <input
            placeholder="Enter name"
        />
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.jsx")?;
    // Multi-line attribute format should use // comment
    assert!(
        content.contains("// glot-disable-next-line hardcoded"),
        "Expected JS comment for multi-line attribute, got:\n{}",
        content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Should NOT have JSX comment for multi-line attribute, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_in_jsx_file_uses_jsx_comment() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.jsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <div>{t("greeting")}</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.jsx")?;
    assert!(
        content.contains("{/* glot-disable-next-line untranslated */}"),
        "Expected JSX comment in .jsx file, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_jsx_children_uses_jsx_comment() -> Result<()> {
    // t() call as direct JSX child on its own line
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <div>
            {t("greeting")}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("{/* glot-disable-next-line untranslated */}"),
        "Expected JSX comment for t() in JSX children, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_jsx_attribute_uses_slash_comment() -> Result<()> {
    // t() call in JSX attribute
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <input placeholder={t("placeholder")} />;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"placeholder": "Enter text"}}"#,
    )?;
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"placeholder": "Enter text"}}"#,
    )?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("// glot-disable-next-line untranslated"),
        "Expected JS comment for t() in attribute, got:\n{}",
        content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line untranslated */}"),
        "Should NOT have JSX comment for attribute, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_multiline_attribute_uses_slash_comment() -> Result<()> {
    // t() call in multi-line attribute format
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <input
            placeholder={t("placeholder")}
        />
    );
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{"Common": {"placeholder": "Enter text"}}"#,
    )?;
    test.write_file(
        "messages/zh.json",
        r#"{"Common": {"placeholder": "Enter text"}}"#,
    )?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("// glot-disable-next-line untranslated"),
        "Expected JS comment for multi-line attribute, got:\n{}",
        content
    );
    assert!(
        !content.contains("{/* glot-disable-next-line untranslated */}"),
        "Should NOT have JSX comment for multi-line attribute, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_hardcoded_ternary_same_line_uses_jsx_comment() -> Result<()> {
    // Ternary with JSX on same line in JSX children should use JSX comment
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App({ isActive }) {
    return (
        <div>
            {isActive ? <span>Active</span> : <span>Inactive</span>}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Comment goes in JSX children position before the line
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment for ternary on same line, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_map_callback_jsx_uses_jsx_comment() -> Result<()> {
    // t() inside map callback returning JSX, text on its own line in JSX element
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App({ items }) {
    const t = useTranslations("Common");
    return (
        <ul>
            {items.map((item) => (
                <li key={item.id}>
                    {t("item_label")}
                </li>
            ))}
        </ul>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"item_label": "Item"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"item_label": "Item"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // t() is on its own line inside <li>, which is JSX children context
    assert!(
        content.contains("{/* glot-disable-next-line untranslated */}"),
        "Expected JSX comment for t() on own line in JSX element, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_hardcoded_jsx_text_on_own_line_uses_jsx_comment() -> Result<()> {
    // Hardcoded text on its own line inside element should use JSX comment
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"export function App() {
    return (
        <div>
            <p>
                This is standalone text
            </p>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Text on own line inside <p> should use JSX comment
    assert!(
        content.contains("{/* glot-disable-next-line hardcoded */}"),
        "Expected JSX comment for text on own line, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_in_conditional_jsx_return_uses_jsx_comment() -> Result<()> {
    // t() in conditional expression that returns JSX element
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App({ show }) {
    const t = useTranslations("Common");
    return (
        <div>
            {show && (
                <span>
                    {t("message")}
                </span>
            )}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"message": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"message": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // t() is inside <span> children, should use JSX comment
    assert!(
        content.contains("{/* glot-disable-next-line untranslated */}"),
        "Expected JSX comment for t() in nested JSX element, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_skips_line_with_translation_call() -> Result<()> {
    // Line has both t("key") AND hardcoded text "(Hardcoded)"
    // Should skip inserting comment to avoid making key appear unused
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <span>{t("title")} (Hardcoded)</span>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"title": "Title"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // File should NOT be modified - no comment inserted
    let content = test.read_file("src/app.tsx")?;
    assert!(
        !content.contains("glot-disable-next-line"),
        "Should NOT insert comment when line has translation call:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_skips_and_inserts_mixed() -> Result<()> {
    // Test that some lines are skipped while others are inserted
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <div>
            <span>{t("title")} (Hardcoded suffix)</span>
            <span>Pure hardcoded text</span>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"title": "Title"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Should have ONE comment for "Pure hardcoded text"
    let comment_count = content.matches("glot-disable-next-line").count();
    assert_eq!(
        comment_count, 1,
        "Expected 1 comment (skipped line with t()), got {}:\n{}",
        comment_count, content
    );
    Ok(())
}

// ============================================================
// Untranslated Rule Tests
// ============================================================

#[test]
fn test_baseline_untranslated_single_usage() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <div>{t("greeting")}</div>;
}
"#,
    )?;

    // Same value in en and zh = untranslated
    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    assert_cmd_snapshot!(test.baseline_command());
    Ok(())
}

#[test]
fn test_baseline_untranslated_apply() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <div>{t("greeting")}</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify comment was inserted
    let content = test.read_file("src/app.tsx")?;
    assert!(
        content.contains("glot-disable-next-line untranslated"),
        "Expected untranslated comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_untranslated_multiple_usages() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Same key used in two different files
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <div>{t("greeting")}</div>;
}
"#,
    )?;

    test.write_file(
        "src/other.tsx",
        r#"import { useTranslations } from "next-intl";
export function Other() {
    const t = useTranslations("Common");
    return <span>{t("greeting")}</span>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify comments were inserted in both files
    let app_content = test.read_file("src/app.tsx")?;
    let other_content = test.read_file("src/other.tsx")?;

    assert!(
        app_content.contains("glot-disable-next-line untranslated"),
        "Expected untranslated comment in app.tsx, got:\n{}",
        app_content
    );
    assert!(
        other_content.contains("glot-disable-next-line untranslated"),
        "Expected untranslated comment in other.tsx, got:\n{}",
        other_content
    );
    Ok(())
}

#[test]
fn test_baseline_rule_hardcoded_only() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // File has both hardcoded text and uses untranslated key
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <div>
            <span>Hardcoded text</span>
            <span>{t("greeting")}</span>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    // Only process hardcoded rule
    let mut cmd = test.baseline_command();
    cmd.args(["--apply", "--rules", "hardcoded"]);
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Should have hardcoded comment
    assert!(
        content.contains("glot-disable-next-line hardcoded"),
        "Expected hardcoded comment, got:\n{}",
        content
    );
    // Should NOT have untranslated comment
    assert!(
        !content.contains("untranslated"),
        "Should not have untranslated comment when --rule hardcoded, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_rule_untranslated_only() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <div>
            <span>Hardcoded text</span>
            <span>{t("greeting")}</span>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    // Only process untranslated rule
    let mut cmd = test.baseline_command();
    cmd.args(["--apply", "--rules", "untranslated"]);
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // Should have untranslated comment
    assert!(
        content.contains("glot-disable-next-line untranslated"),
        "Expected untranslated comment, got:\n{}",
        content
    );
    // Should NOT have hardcoded comment (only untranslated was requested)
    let hardcoded_count = content.matches("hardcoded").count();
    assert_eq!(
        hardcoded_count, 0,
        "Should not have hardcoded comment when --rule untranslated, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_mixed_rules_same_line() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Line has both hardcoded text AND uses untranslated key
    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <div>{t("greeting")} suffix</div>;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    // The hardcoded "suffix" is on a line with t() call, so it should be skipped for hardcoded
    // But untranslated should still be inserted for the t("greeting") usage
    assert!(
        content.contains("glot-disable-next-line untranslated"),
        "Expected untranslated comment, got:\n{}",
        content
    );
    Ok(())
}

#[test]
fn test_baseline_no_untranslated_when_properly_translated() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return <div>{t("greeting")}</div>;
}
"#,
    )?;

    // Different values = properly translated
    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "你好"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.args(["--rules", "untranslated"]);
    assert_cmd_snapshot!(cmd);
    Ok(())
}

#[test]
fn test_baseline_stats_output() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <div>
            <span>Hardcoded one</span>
            <span>Hardcoded two</span>
            <span>{t("greeting")}</span>
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    // Dry-run to see stats
    assert_cmd_snapshot!(test.baseline_command());
    Ok(())
}
