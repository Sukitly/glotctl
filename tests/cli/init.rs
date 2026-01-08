use anyhow::Result;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn test_init_creates_config() -> Result<()> {
    let test = CliTest::new()?;

    assert_cmd_snapshot!(test.command().arg("init"));

    assert!(test.root().join(".glotrc.json").exists());

    Ok(())
}

#[test]
fn test_init_fails_if_exists() -> Result<()> {
    let test = CliTest::new()?;
    test.write_file(".glotrc.json", "{}")?;

    assert_cmd_snapshot!(test.command().arg("init"));

    Ok(())
}
