use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Ok, Result};
use insta_cmd::get_cargo_bin;
use tempfile::TempDir;

mod baseline;
mod check;
mod clean;
mod fix;
mod init;

const BIN_NAME: &str = "glot";

pub struct CliTest {
    _temp_dir: TempDir,
    project_dir: PathBuf,
}

impl CliTest {
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().canonicalize()?;
        Ok(Self {
            _temp_dir: temp_dir,
            project_dir,
        })
    }

    pub fn with_file(path: &str, content: &str) -> Result<Self> {
        let test = Self::new()?;
        test.write_file(path, content)?;
        Ok(test)
    }

    pub fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let file_path = self.project_dir.join(path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory:{}", parent.display()))?;
        }

        fs::write(&file_path, content)
            .with_context(|| format!("Failed to write file: {}", file_path.display()))?;

        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.project_dir
    }

    pub fn command(&self) -> Command {
        let mut cmd = Command::new(get_cargo_bin(BIN_NAME));
        cmd.current_dir(&self.project_dir);
        cmd.env_clear();
        cmd.env("NO_COLOR", "1"); // Disable colors for consistent test output
        cmd.env("GLOT_DISABLE_TIMING", "1"); // Disable timing for stable snapshots
        cmd
    }

    pub fn check_command(&self) -> Command {
        let mut cmd = self.command();
        cmd.arg("check");
        cmd
    }

    pub fn clean_command(&self) -> Command {
        let mut cmd = self.command();
        cmd.arg("clean");
        cmd
    }

    pub fn baseline_command(&self) -> Command {
        let mut cmd = self.command();
        cmd.arg("baseline");
        cmd
    }

    pub fn fix_command(&self) -> Command {
        let mut cmd = self.command();
        cmd.arg("fix");
        cmd
    }

    pub fn read_file(&self, path: &str) -> Result<String> {
        let file_path = self.project_dir.join(path);
        fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))
    }
}
