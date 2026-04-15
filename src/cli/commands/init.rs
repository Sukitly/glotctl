use std::{fs, path::Path};

use anyhow::Result;
use colored::Colorize;

use super::super::exit_status::ExitStatus;
use super::super::report::SUCCESS_MARK;
use crate::config::{CONFIG_FILE_NAME, Framework, default_config_json};

/// Detect the i18n framework by inspecting `package.json` dependencies.
fn detect_framework() -> Framework {
    let Ok(content) = fs::read_to_string("package.json") else {
        return Framework::default();
    };
    let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Framework::default();
    };

    let has_dep = |name: &str| -> bool {
        pkg.get("dependencies").and_then(|d| d.get(name)).is_some()
            || pkg
                .get("devDependencies")
                .and_then(|d| d.get(name))
                .is_some()
    };

    if has_dep("next-intl") {
        Framework::NextIntl
    } else {
        // Default to react-i18next (covers react-i18next, i18next, or no match)
        Framework::ReactI18next
    }
}

pub fn init() -> Result<ExitStatus> {
    let config_path = Path::new(CONFIG_FILE_NAME);

    if config_path.exists() {
        eprintln!("Error: {} already exists", CONFIG_FILE_NAME);
        return Ok(ExitStatus::Failure);
    }

    let framework = detect_framework();
    fs::write(config_path, default_config_json(framework)?)?;

    let framework_label = match framework {
        Framework::NextIntl => "next-intl",
        Framework::ReactI18next => "react-i18next",
    };

    println!(
        "{} {}",
        SUCCESS_MARK.green(),
        format!(
            "Created {} (detected framework: {})",
            CONFIG_FILE_NAME, framework_label
        )
        .green()
    );

    Ok(ExitStatus::Success)
}
