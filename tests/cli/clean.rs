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
fn test_clean_dry_run_shows_unused_keys() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file that uses only one key
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("used")}</div>;
}
"#,
    )?;

    // Create message file with used and unused keys
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "used": "Used Key",
    "unused": "Unused Key"
  }
}"#,
    )?;

    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_no_keys_to_clean() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file that uses all keys
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("submit")}{t("cancel")}</div>;
}
"#,
    )?;

    // Create message file with all keys used
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "submit": "Submit",
    "cancel": "Cancel"
  }
}"#,
    )?;

    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_refuses_with_dynamic_key() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file with a dynamic key
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App({ keyName }: { keyName: string }) {
    return <div>{t(keyName)}</div>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "submit": "Submit"
  }
}"#,
    )?;

    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_apply_removes_keys() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file that uses only one key
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("used")}</div>;
}
"#,
    )?;

    // Create message file with used and unused keys
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "used": "Used Key",
    "unused": "Unused Key"
  }
}"#,
    )?;

    // Run with --apply
    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify the file was modified
    let content = test.read_file("messages/en.json")?;
    assert!(content.contains("\"used\""));
    assert!(!content.contains("\"unused\""));
    Ok(())
}

#[test]
fn test_clean_orphan_keys() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file that uses a key
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("submit")}</div>;
}
"#,
    )?;

    // Primary locale has the key
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "submit": "Submit"
  }
}"#,
    )?;

    // Secondary locale has an orphan key
    test.write_file(
        "messages/zh.json",
        r#"{
  "Common": {
    "submit": "Submit",
    "orphan": "Orphan Key"
  }
}"#,
    )?;

    // Run with --orphan flag
    let mut cmd = test.clean_command();
    cmd.arg("--orphan");
    assert_cmd_snapshot!(cmd);
    Ok(())
}

#[test]
fn test_clean_unused_only() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("submit")}</div>;
}
"#,
    )?;

    // Primary locale has unused key
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "submit": "Submit",
    "unused": "Unused"
  }
}"#,
    )?;

    // Secondary locale has orphan key
    test.write_file(
        "messages/zh.json",
        r#"{
  "Common": {
    "submit": "Submit",
    "orphan": "Orphan"
  }
}"#,
    )?;

    // Run with --unused flag - should only show unused, not orphan
    let mut cmd = test.clean_command();
    cmd.arg("--unused");
    assert_cmd_snapshot!(cmd);
    Ok(())
}

#[test]
fn test_clean_removes_empty_parent() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Create a TSX file that doesn't use any Common keys
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Other");
export function App() {
    return <div>{t("key")}</div>;
}
"#,
    )?;

    // Create message file where Common will become empty after clean
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "unused": "Unused Key"
  },
  "Other": {
    "key": "Used Key"
  }
}"#,
    )?;

    // Run with --apply
    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify Common was removed entirely
    let content = test.read_file("messages/en.json")?;
    assert!(!content.contains("\"Common\""));
    assert!(content.contains("\"Other\""));
    Ok(())
}

// #[test]
// fn test_clean_unused_propagates_to_all_locales() -> Result<()> {
//     let test = CliTest::new()?;
//     setup_config(&test)?;

//     // Code only uses "used" key, not "unused"
//     test.write_file(
//         "src/app.tsx",
//         r#"
// const t = useTranslations("Common");
// export function App() {
//     return <div>{t("used")}</div>;
// }
// "#,
//     )?;

//     // Both locales have the unused key
//     test.write_file(
//         "messages/en.json",
//         r#"{
//   "Common": {
//     "used": "Used",
//     "unused": "Unused"
//   }
// }"#,
//     )?;

//     test.write_file(
//         "messages/zh.json",
//         r#"{
//   "Common": {
//     "used": "已使用",
//     "unused": "未使用"
//   }
// }"#,
//     )?;

//     // Should show unused in BOTH locales
//     assert_cmd_snapshot!(test.clean_command());
//     Ok(())
// }

#[test]
fn test_clean_apply_removes_from_all_locales() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Code only uses "used" key
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("used")}</div>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "used": "Used",
    "unused": "Unused"
  }
}"#,
    )?;

    test.write_file(
        "messages/zh.json",
        r#"{
  "Common": {
    "used": "已使用",
    "unused": "未使用"
  }
}"#,
    )?;

    // Run with --apply
    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify both files were modified
    let en_content = test.read_file("messages/en.json")?;
    assert!(en_content.contains("\"used\""));
    assert!(!en_content.contains("\"unused\""));

    let zh_content = test.read_file("messages/zh.json")?;
    assert!(zh_content.contains("\"used\""));
    assert!(!zh_content.contains("\"unused\""));

    Ok(())
}

#[test]
fn test_clean_handles_compact_json() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // Code only uses "used" key
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("used")}</div>;
}
"#,
    )?;

    // Compact JSON (multiple keys on same line)
    test.write_file(
        "messages/en.json",
        r#"{"Common": {"used": "Used", "unused": "Unused"}}"#,
    )?;

    // Run with --apply
    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify the unused key was removed and file was reformatted
    let content = test.read_file("messages/en.json")?;
    assert!(content.contains("\"used\""));
    assert!(!content.contains("\"unused\""));

    Ok(())
}

#[test]
fn test_clean_refuses_with_invalid_json() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("used")}</div>;
}
"#,
    )?;

    // Valid primary locale
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "used": "Used",
    "unused": "Unused"
  }
}"#,
    )?;

    // Invalid JSON in secondary locale
    test.write_file("messages/zh.json", r#"{ invalid json }"#)?;

    // Should refuse to clean due to parse error
    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_messages_dir_not_found() -> Result<()> {
    let test = CliTest::new()?;

    // Config points to non-existent messages directory
    test.write_file(
        ".glotrc.json",
        r#"{
            "includes": ["src"],
            "messagesDir": "./nonexistent",
            "primaryLocale": "en"
        }"#,
    )?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("submit")}</div>;
}
"#,
    )?;

    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_primary_locale_not_found() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("submit")}</div>;
}
"#,
    )?;

    // Only secondary locale exists, no primary (en.json)
    test.write_file(
        "messages/zh.json",
        r#"{
  "Common": {
    "submit": "提交"
  }
}"#,
    )?;

    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_orphan_apply() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("submit")}</div>;
}
"#,
    )?;

    // Primary locale has the key
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "submit": "Submit"
  }
}"#,
    )?;

    // Secondary locale has an orphan key
    test.write_file(
        "messages/zh.json",
        r#"{
  "Common": {
    "submit": "提交",
    "orphan": "孤儿"
  }
}"#,
    )?;

    // Run with --orphan --apply
    let mut cmd = test.clean_command();
    cmd.arg("--orphan").arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify orphan was removed
    let content = test.read_file("messages/zh.json")?;
    assert!(content.contains("\"submit\""));
    assert!(!content.contains("\"orphan\""));

    Ok(())
}

#[test]
fn test_clean_deeply_nested_keys() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("App.Settings.Account");
export function App() {
    return <div>{t("used")}</div>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
  "App": {
    "Settings": {
      "Account": {
        "used": "Used",
        "unused": "Unused"
      }
    }
  }
}"#,
    )?;

    // Dry run
    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}

#[test]
fn test_clean_deeply_nested_apply_removes_empty_parents() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // No keys used from App.Settings.Account
    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Other");
export function App() {
    return <div>{t("key")}</div>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
  "App": {
    "Settings": {
      "Account": {
        "unused": "Unused"
      }
    }
  },
  "Other": {
    "key": "Key"
  }
}"#,
    )?;

    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify entire App tree was removed
    let content = test.read_file("messages/en.json")?;
    assert!(!content.contains("\"App\""));
    assert!(!content.contains("\"Settings\""));
    assert!(!content.contains("\"Account\""));
    assert!(content.contains("\"Other\""));

    Ok(())
}

#[test]
fn test_clean_preserves_key_order() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t = useTranslations("Common");
export function App() {
    return <div>{t("alpha")}{t("gamma")}</div>;
}
"#,
    )?;

    // Keys in non-alphabetical order: z, a, m, b
    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "zebra": "Zebra",
    "alpha": "Alpha",
    "middle": "Middle",
    "gamma": "Gamma"
  }
}"#,
    )?;

    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify key order is preserved (alpha should come before gamma)
    let content = test.read_file("messages/en.json")?;
    let alpha_pos = content.find("\"alpha\"").unwrap();
    let gamma_pos = content.find("\"gamma\"").unwrap();
    assert!(
        alpha_pos < gamma_pos,
        "Key order should be preserved: alpha before gamma"
    );

    Ok(())
}

#[test]
fn test_clean_all_keys_unused() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    // No translation calls
    test.write_file(
        "src/app.tsx",
        r#"
export function App() {
    return <div>Hello</div>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
  "Common": {
    "unused1": "Unused 1",
    "unused2": "Unused 2"
  }
}"#,
    )?;

    let mut cmd = test.clean_command();
    cmd.arg("--apply");
    assert_cmd_snapshot!(cmd);

    // Verify file becomes empty object
    let content = test.read_file("messages/en.json")?;
    assert!(!content.contains("\"Common\""));
    assert!(!content.contains("\"unused1\""));

    Ok(())
}

#[test]
fn test_clean_multiple_namespaces_mixed() -> Result<()> {
    let test = CliTest::new()?;
    setup_config(&test)?;

    test.write_file(
        "src/app.tsx",
        r#"
const t1 = useTranslations("Auth");
const t2 = useTranslations("Common");
export function App() {
    return <div>{t1("login")}{t2("save")}</div>;
}
"#,
    )?;

    test.write_file(
        "messages/en.json",
        r#"{
  "Auth": {
    "login": "Login",
    "logout": "Logout"
  },
  "Common": {
    "save": "Save",
    "cancel": "Cancel"
  },
  "Unused": {
    "key": "Value"
  }
}"#,
    )?;

    assert_cmd_snapshot!(test.clean_command());
    Ok(())
}
