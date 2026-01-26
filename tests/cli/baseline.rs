use anyhow::Result;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

const JSX_HARDCODED: &str = "{/* glot-disable-next-line hardcoded */}";
const JS_HARDCODED: &str = "// glot-disable-next-line hardcoded";
const JSX_UNTRANSLATED: &str = "{/* glot-disable-next-line untranslated */}";
const JS_UNTRANSLATED: &str = "// glot-disable-next-line untranslated";
const JSX_MERGED: &str = "{/* glot-disable-next-line hardcoded untranslated */}";
const JS_MERGED: &str = "// glot-disable-next-line hardcoded untranslated";

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

fn assert_comment_insertions(content: &str, comment: &str, line_contains: &[&str]) {
    let count = content.matches(comment).count();
    assert_eq!(
        count,
        line_contains.len(),
        "Expected {} comment(s) '{}' but found {}:\n{}",
        line_contains.len(),
        comment,
        count,
        content
    );

    let lines: Vec<&str> = content.lines().collect();
    for target in line_contains {
        let mut found = false;
        for (idx, line) in lines.iter().enumerate() {
            if line.contains(target) {
                found = true;
                assert!(
                    idx > 0,
                    "Expected comment before line containing '{}' but it is on the first line:\n{}",
                    target,
                    content
                );
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                let expected = format!("{indent}{comment}");
                let actual = lines[idx - 1].to_string();
                assert_eq!(
                    actual, expected,
                    "Expected comment before line containing '{}':\n{}",
                    target, content
                );
                break;
            }
        }
        assert!(
            found,
            "Expected line containing '{}' but it was not found:\n{}",
            target, content
        );
    }
}

fn assert_no_comments(content: &str) {
    for comment in [
        JSX_HARDCODED,
        JS_HARDCODED,
        JSX_UNTRANSLATED,
        JS_UNTRANSLATED,
        JSX_MERGED,
        JS_MERGED,
    ] {
        assert!(
            !content.contains(comment),
            "Expected no '{}' comment, got:\n{}",
            comment,
            content
        );
    }
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

    let content = test.read_file("src/app.tsx")?;
    assert_comment_insertions(&content, JSX_HARDCODED, &["return <div>Hello World</div>;"]);
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

    let content = test.read_file("src/app.tsx")?;
    assert_comment_insertions(
        &content,
        JS_HARDCODED,
        &["return <input placeholder=\"Enter name\" />;"],
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
    let content = test.read_file("src/app.tsx")?;
    assert_no_comments(&content);
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["return <div>Hello World</div>;"]);
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["<span>Deeply indented</span>"]);
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
    assert_comment_insertions(
        &content,
        JSX_HARDCODED,
        &["<nav aria-label=\"Tabs\">content</nav>"],
    );
    assert!(
        !content.contains(JS_HARDCODED),
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
    assert_comment_insertions(&content, JS_HARDCODED, &["placeholder=\"Enter name\""]);
    assert!(
        !content.contains(JSX_HARDCODED),
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["<span>JSX Text</span>"]);
    assert_comment_insertions(&content, JS_HARDCODED, &["placeholder=\"Multi-line attr\""]);
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
    assert_comment_insertions(
        &content,
        JS_HARDCODED,
        &["<p>No console output</p>", "<span>Has output</span>"],
    );
    assert!(
        !content.contains(JSX_HARDCODED),
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["<span>Fragment child</span>"]);
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
    assert_comment_insertions(
        &content,
        JSX_HARDCODED,
        &["{show && <span>Conditional text</span>}"],
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
    assert_comment_insertions(&content, JS_HARDCODED, &["? \"Processing\"", ": \"Ready\""]);
    assert!(
        !content.contains(JSX_HARDCODED),
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
    assert_comment_insertions(
        &content,
        JS_UNTRANSLATED,
        &["? t(\"processing\")", ": t(\"ready\")"],
    );
    assert!(
        !content.contains(JSX_UNTRANSLATED),
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
    assert_comment_insertions(
        &content,
        JS_HARDCODED,
        &["<span key={item.id}>Item text</span>"],
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["Nested text"]);
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
    assert_comment_insertions(
        &content,
        JS_UNTRANSLATED,
        &["const label = t(\"greeting\");"],
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
    assert_comment_insertions(
        &content,
        JS_UNTRANSLATED,
        &["const label = t(\"greeting\");"],
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["return <div>Hardcoded</div>;"]);
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
    assert_comment_insertions(
        &content,
        JS_HARDCODED,
        &["return <input placeholder=\"Enter name\" />;"],
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
    assert_comment_insertions(&content, JS_HARDCODED, &["placeholder=\"Enter name\""]);
    assert!(
        !content.contains(JSX_HARDCODED),
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
    assert_comment_insertions(
        &content,
        JSX_UNTRANSLATED,
        &["return <div>{t(\"greeting\")}</div>;"],
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
    assert_comment_insertions(&content, JSX_UNTRANSLATED, &["{t(\"greeting\")}"]);
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
    assert_comment_insertions(
        &content,
        JS_UNTRANSLATED,
        &["return <input placeholder={t(\"placeholder\")} />;"],
    );
    assert!(
        !content.contains(JSX_UNTRANSLATED),
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
    assert_comment_insertions(
        &content,
        JS_UNTRANSLATED,
        &["placeholder={t(\"placeholder\")}"],
    );
    assert!(
        !content.contains(JSX_UNTRANSLATED),
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
    assert_comment_insertions(
        &content,
        JSX_HARDCODED,
        &["{isActive ? <span>Active</span> : <span>Inactive</span>}"],
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
    assert_comment_insertions(&content, JSX_UNTRANSLATED, &["{t(\"item_label\")}"]);
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["This is standalone text"]);
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
    assert_comment_insertions(&content, JSX_UNTRANSLATED, &["{t(\"message\")}"]);
    Ok(())
}

#[test]
fn test_baseline_skips_line_with_translation_call() -> Result<()> {
    // Line has both t("key") AND hardcoded text "(Hardcoded)"
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

    let content = test.read_file("src/app.tsx")?;
    assert_comment_insertions(
        &content,
        JSX_HARDCODED,
        &["<span>{t(\"title\")} (Hardcoded)</span>"],
    );
    Ok(())
}

#[test]
fn test_baseline_skips_and_inserts_mixed() -> Result<()> {
    // Comments should be inserted for both lines
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
    assert_comment_insertions(
        &content,
        JSX_HARDCODED,
        &[
            "<span>{t(\"title\")} (Hardcoded suffix)</span>",
            "<span>Pure hardcoded text</span>",
        ],
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
    let content = test.read_file("src/app.tsx")?;
    assert_no_comments(&content);
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

    let content = test.read_file("src/app.tsx")?;
    assert_comment_insertions(
        &content,
        JSX_UNTRANSLATED,
        &["return <div>{t(\"greeting\")}</div>;"],
    );
    Ok(())
}

#[test]
fn test_baseline_merges_existing_disable_comment_js() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.ts",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    // glot-disable-next-line hardcoded
    const label = t("greeting");
    return label;
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.args(["--apply", "--rules", "untranslated"]);
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.ts")?;
    assert_comment_insertions(&content, JS_MERGED, &["const label = t(\"greeting\");"]);
    Ok(())
}

#[test]
fn test_baseline_merges_existing_disable_comment_jsx() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"import { useTranslations } from "next-intl";
export function App() {
    const t = useTranslations("Common");
    return (
        <div>
            {/* glot-disable-next-line hardcoded */}
            {t("greeting")}
        </div>
    );
}
"#,
    )?;

    test.write_file("messages/en.json", r#"{"Common": {"greeting": "Hello"}}"#)?;
    test.write_file("messages/zh.json", r#"{"Common": {"greeting": "Hello"}}"#)?;

    let mut cmd = test.baseline_command();
    cmd.args(["--apply", "--rules", "untranslated"]);
    assert_cmd_snapshot!(cmd);

    let content = test.read_file("src/app.tsx")?;
    assert_comment_insertions(&content, JSX_MERGED, &["{t(\"greeting\")}"]);
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

    assert_comment_insertions(
        &app_content,
        JSX_UNTRANSLATED,
        &["return <div>{t(\"greeting\")}</div>;"],
    );
    assert_comment_insertions(
        &other_content,
        JSX_UNTRANSLATED,
        &["return <span>{t(\"greeting\")}</span>;"],
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
    assert_comment_insertions(&content, JSX_HARDCODED, &["<span>Hardcoded text</span>"]);
    // Should NOT have untranslated comment
    assert!(
        !content.contains(JSX_UNTRANSLATED) && !content.contains(JS_UNTRANSLATED),
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
    assert_comment_insertions(
        &content,
        JSX_UNTRANSLATED,
        &["<span>{t(\"greeting\")}</span>"],
    );
    // Should NOT have hardcoded comment (only untranslated was requested)
    assert!(
        !content.contains(JSX_HARDCODED) && !content.contains(JS_HARDCODED),
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
    assert_comment_insertions(
        &content,
        JSX_MERGED,
        &["return <div>{t(\"greeting\")} suffix</div>;"],
    );
    assert!(
        !content.contains(JSX_HARDCODED),
        "Expected merged comment, got:\n{}",
        content
    );
    assert!(
        !content.contains(JSX_UNTRANSLATED),
        "Expected merged comment, got:\n{}",
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
    let content = test.read_file("src/app.tsx")?;
    assert_no_comments(&content);
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
    let content = test.read_file("src/app.tsx")?;
    assert_no_comments(&content);
    Ok(())
}
