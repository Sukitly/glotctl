use std::{fs, path::Path};

use anyhow::Result;
use colored::Colorize;

use super::super::exit_status::ExitStatus;
use super::super::report::SUCCESS_MARK;
use crate::config::{CONFIG_FILE_NAME, default_config_json};

pub fn init() -> Result<ExitStatus> {
    let config_path = Path::new(CONFIG_FILE_NAME);

    if config_path.exists() {
        eprintln!("Error: {} already exists", CONFIG_FILE_NAME);
        return Ok(ExitStatus::Failure);
    }

    fs::write(config_path, default_config_json()?)?;
    println!(
        "{} {}",
        SUCCESS_MARK.green(),
        format!("Created {}", CONFIG_FILE_NAME).green()
    );

    Ok(ExitStatus::Success)
}
