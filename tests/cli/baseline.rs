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
