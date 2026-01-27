use anyhow::{Context, Result};
use insta_cmd::assert_cmd_snapshot;
use serde_json::Value;

use crate::CliTest;

/// Validates config file structure and default values.
fn assert_config_content(content: &str) -> Result<()> {
    // 1. Parse as JSON
    let parsed: Value = serde_json::from_str(content).context("Config should be valid JSON")?;

    // 2. Verify expected fields exist
    assert!(
        parsed.get("includes").is_some(),
        "Config should have 'includes' field"
    );
    assert!(
        parsed.get("messagesRoot").is_some(),
        "Config should have 'messagesRoot' field"
    );
    assert!(
        parsed.get("primaryLocale").is_some(),
        "Config should have 'primaryLocale' field"
    );

    // 3. Verify formatting (2-space indentation)
    assert!(
        content.contains("  "),
        "Config should use 2-space indentation"
    );

    Ok(())
}

#[test]
fn test_init_creates_config() -> Result<()> {
    let test = CliTest::new()?;

    assert_cmd_snapshot!(test.command().arg("init"));

    // Verify file exists
    assert!(test.root().join(".glotrc.json").exists());

    // Verify content is valid and has expected structure
    let content = test.read_file(".glotrc.json")?;
    assert_config_content(&content)?;

    Ok(())
}

#[test]
fn test_init_fails_if_exists() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(".glotrc.json", "{}")?;

    assert_cmd_snapshot!(test.command().arg("init"));

    Ok(())
}

#[test]
fn test_init_config_is_immediately_usable() -> Result<()> {
    let test = CliTest::new()?;

    // Initialize config
    test.command().arg("init").output()?;

    // Create minimal project structure
    test.write_file(
        "src/app.tsx",
        r#"export function App() { return <div>Test</div>; }"#,
    )?;
    test.write_file("messages/en.json", "{}")?;

    // Verify check command works with initialized config
    let output = test.check_command().output()?;
    assert!(
        output.status.success(),
        "Check command should work with initialized config. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}
