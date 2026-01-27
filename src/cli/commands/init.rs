use std::{fs, path::Path};

use super::{CommandResult, CommandSummary, InitSummary};
use crate::config::{default_config_json, CONFIG_FILE_NAME};
use anyhow::Result;

pub fn init() -> Result<CommandResult> {
    let config_path = Path::new(CONFIG_FILE_NAME);
    if config_path.exists() {
        return Ok(CommandResult {
            summary: CommandSummary::Init(InitSummary {
                created: false,
                error: Some(format!("{} already exists", CONFIG_FILE_NAME)),
            }),
            error_count: 1,
            exit_on_errors: true,
            issues: Vec::new(),
            parse_error_count: 0,
            source_files_checked: 0,
            locale_files_checked: 0,
        });
    }

    fs::write(config_path, default_config_json()?)?;
    Ok(CommandResult {
        summary: CommandSummary::Init(InitSummary {
            created: true,
            error: None,
        }),
        error_count: 0,
        exit_on_errors: true,
        issues: Vec::new(),
        parse_error_count: 0,
        source_files_checked: 0,
        locale_files_checked: 0,
    })
}
