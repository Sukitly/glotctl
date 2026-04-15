//! Configuration file loading and parsing.
//!
//! This module handles loading configuration from `.glotrc.json` files
//! and provides default configuration values.
//!
//! ## Configuration Priority
//!
//! 1. CLI arguments (highest priority)
//! 2. .glotrc.json file
//! 3. Built-in defaults (lowest priority)

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Ok, Result};
use glob::Pattern;
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE_NAME: &str = ".glotrc.json";

/// Supported i18n framework.
#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    ReactI18next,
    #[default]
    NextIntl,
}

pub const TEST_FILE_PATTERNS: &[&str] = &[
    "**/*.test.tsx",
    "**/*.test.ts",
    "**/*.test.jsx",
    "**/*.test.js",
    "**/*.spec.tsx",
    "**/*.spec.ts",
    "**/*.spec.jsx",
    "**/*.spec.js",
    "**/__tests__/**",
];

/// Raw config as deserialized from JSON. Framework-dependent fields use Option
/// to distinguish "not set" from "explicitly set".
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawConfig {
    #[serde(default)]
    framework: Framework,
    #[serde(default)]
    ignores: Vec<String>,
    includes: Option<Vec<String>>,
    #[serde(default = "default_checked_attributes")]
    checked_attributes: Vec<String>,
    #[serde(default)]
    ignore_texts: Vec<String>,
    #[serde(alias = "messagesDir")]
    messages_root: Option<String>,
    #[serde(default = "default_primary_locale")]
    primary_locale: String,
    source_root: Option<String>,
    #[serde(default = "default_ignore_test_files")]
    ignore_test_files: bool,
}

impl RawConfig {
    /// Resolve framework-dependent defaults for fields that were not explicitly set.
    fn into_config(self) -> Config {
        let fw = self.framework;
        Config {
            framework: fw,
            ignores: self.ignores,
            includes: self.includes.unwrap_or_else(|| default_includes_for(fw)),
            checked_attributes: self.checked_attributes,
            ignore_texts: self.ignore_texts,
            messages_root: self
                .messages_root
                .unwrap_or_else(|| default_messages_root_for(fw)),
            primary_locale: self.primary_locale,
            source_root: self.source_root.unwrap_or_else(default_source_root),
            ignore_test_files: self.ignore_test_files,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub framework: Framework,
    #[serde(default)]
    pub ignores: Vec<String>,
    #[serde(default = "default_includes")]
    pub includes: Vec<String>,
    #[serde(default = "default_checked_attributes")]
    pub checked_attributes: Vec<String>,
    #[serde(default)]
    pub ignore_texts: Vec<String>,
    #[serde(default = "default_messages_root", alias = "messagesDir")]
    pub messages_root: String,
    #[serde(default = "default_primary_locale")]
    pub primary_locale: String,
    #[serde(default = "default_source_root")]
    pub source_root: String,
    #[serde(default = "default_ignore_test_files")]
    pub ignore_test_files: bool,
}

fn default_includes() -> Vec<String> {
    default_includes_for(Framework::default())
}

fn default_includes_for(framework: Framework) -> Vec<String> {
    match framework {
        Framework::ReactI18next => vec!["src/components".to_string()],
        Framework::NextIntl => {
            let root_dirs = ["src", ""];
            let sub_dirs = ["app/[locale]", "components"];

            root_dirs
                .iter()
                .flat_map(|root| {
                    sub_dirs.iter().map(move |sub| {
                        if root.is_empty() {
                            sub.to_string()
                        } else {
                            format!("{}/{}", root, sub)
                        }
                    })
                })
                .collect()
        }
    }
}

fn default_checked_attributes() -> Vec<String> {
    [
        "placeholder",
        "title",
        "alt",
        "aria-label",
        "aria-description",
        "aria-placeholder",
        "aria-roledescription",
        "aria-valuetext",
    ]
    .map(String::from)
    .to_vec()
}

fn default_messages_root() -> String {
    default_messages_root_for(Framework::default())
}

fn default_messages_root_for(framework: Framework) -> String {
    match framework {
        Framework::ReactI18next => "./src/locales".to_string(),
        Framework::NextIntl => "./messages".to_string(),
    }
}

fn default_source_root() -> String {
    "./".to_string()
}

fn default_primary_locale() -> String {
    "en".to_string()
}

fn default_ignore_test_files() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self::for_framework(Framework::default())
    }
}

impl Config {
    /// Create a config with defaults appropriate for the given framework.
    pub fn for_framework(framework: Framework) -> Self {
        Self {
            framework,
            ignores: Vec::new(),
            includes: default_includes_for(framework),
            checked_attributes: default_checked_attributes(),
            ignore_texts: Vec::new(),
            messages_root: default_messages_root_for(framework),
            primary_locale: default_primary_locale(),
            source_root: default_source_root(),
            ignore_test_files: default_ignore_test_files(),
        }
    }

    /// Validate configuration values.
    ///
    /// Returns an error if any glob patterns in `ignores` or `includes` are invalid.
    pub fn validate(&self) -> Result<()> {
        // Validate ignore patterns
        for pattern in &self.ignores {
            Pattern::new(pattern)
                .with_context(|| format!("Invalid glob pattern in 'ignores': \"{}\"", pattern))?;
        }

        // Validate include patterns that contain glob wildcards (* or ?)
        // Patterns without wildcards are treated as literal directory paths,
        // so [locale] (Next.js dynamic route) is valid without escaping.
        for pattern in &self.includes {
            if pattern.contains('*') || pattern.contains('?') {
                Pattern::new(pattern).with_context(|| {
                    format!("Invalid glob pattern in 'includes': \"{}\"", pattern)
                })?;
            }
        }

        Ok(())
    }
}

pub fn default_config_json(framework: Framework) -> Result<String> {
    let config = Config::for_framework(framework);
    serde_json::to_string_pretty(&config).context("Failed to generate default config.")
}

pub fn find_config_file(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();

    loop {
        let config_path = current.join(CONFIG_FILE_NAME);
        if config_path.exists() {
            return Some(config_path);
        }
        if current.join(".git").exists() {
            return None;
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Result of loading configuration.
pub struct ConfigLoadResult {
    pub config: Config,
    /// True if config was loaded from a file, false if using defaults.
    pub from_file: bool,
}

pub fn load_config(start_dir: &Path) -> Result<ConfigLoadResult> {
    match find_config_file(start_dir) {
        Some(path) => {
            let content = fs::read_to_string(&path)?;
            let raw: RawConfig = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {:?}", path))?;
            let config = raw.into_config();
            config.validate()?;
            Ok(ConfigLoadResult {
                config,
                from_file: true,
            })
        }
        None => Ok(ConfigLoadResult {
            config: Config::default(),
            from_file: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.ignores.is_empty());
        assert!(!config.includes.is_empty());
        assert!(!config.checked_attributes.is_empty());
    }

    #[test]
    fn test_parse_config() {
        let json = r#"{
              "ignores": ["**/dist/**"],
              "includes": ["src/**"],
              "checkedAttributes": ["placeholder"]
          }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.ignores, vec!["**/dist/**"]);
        assert_eq!(config.includes, vec!["src/**"]);
        assert_eq!(config.checked_attributes, vec!["placeholder"]);
    }

    #[test]
    fn test_find_config_file() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("src").join("components");
        fs::create_dir_all(&sub_dir).unwrap();

        let config_path = dir.path().join(CONFIG_FILE_NAME);
        File::create(&config_path).unwrap();

        let found = find_config_file(&sub_dir);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), config_path);
    }

    #[test]
    fn test_find_config_not_found() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();

        let found = find_config_file(dir.path());
        assert!(found.is_none());
    }

    #[test]
    fn test_partial_config() {
        let json = r#"{ "ignores": ["**/dist/**"] }"#;
        let config: Config = serde_json::from_str(json).unwrap();

        assert_eq!(config.ignores, vec!["**/dist/**"]);
        assert_eq!(config.includes, default_includes());
        assert_eq!(config.checked_attributes, default_checked_attributes());
    }

    #[test]
    fn test_load_config_from_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".glotrc.json");

        fs::write(&config_path, r#"{ "ignores": ["**/test/**"] }"#).unwrap();

        let result = load_config(dir.path()).unwrap();
        assert!(result.from_file);
        assert_eq!(result.config.ignores, vec!["**/test/**"]);
    }

    #[test]
    fn test_load_config_default_when_not_found() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();

        let result = load_config(dir.path()).unwrap();
        assert!(!result.from_file);
        assert!(result.config.ignores.is_empty());
        assert_eq!(result.config.includes, default_includes());
    }

    #[test]
    fn test_validate_valid_config() {
        let config = Config {
            ignores: vec!["**/node_modules/**".to_string(), "**/dist/**".to_string()],
            includes: vec!["src".to_string(), "app/**".to_string()],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_ignore_pattern() {
        let config = Config {
            ignores: vec!["[invalid".to_string()], // unclosed bracket
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ignores"));
    }

    #[test]
    fn test_validate_invalid_include_pattern() {
        let config = Config {
            includes: vec!["src/**/[invalid".to_string()], // unclosed bracket with glob wildcard
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("includes"));
    }

    #[test]
    fn test_validate_nextjs_locale_pattern_is_valid() {
        // [locale] without wildcards should be treated as literal path, not glob
        let config = Config {
            includes: vec!["app/[locale]".to_string()],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_load_config_with_invalid_pattern_fails() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".glotrc.json");

        fs::write(&config_path, r#"{ "ignores": ["[invalid"] }"#).unwrap();

        let result = load_config(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_backward_compatibility_messages_dir() {
        let json = r#"{ "messagesDir": "./locales" }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.messages_root, "./locales");
    }

    #[test]
    fn test_new_messages_root_field() {
        let json = r#"{ "messagesRoot": "./i18n" }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.messages_root, "./i18n");
    }

    #[test]
    fn test_source_root_default() {
        let config = Config::default();
        assert_eq!(config.source_root, "./");
    }

    #[test]
    fn test_serialization_uses_new_names() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("messagesRoot"));
        assert!(!json.contains("messagesDir"));
    }

    // ============================================================
    // Framework tests
    // ============================================================

    #[test]
    fn test_framework_serialization_kebab_case() {
        let json = serde_json::to_string(&Framework::ReactI18next).unwrap();
        assert_eq!(json, r#""react-i18next""#);

        let json = serde_json::to_string(&Framework::NextIntl).unwrap();
        assert_eq!(json, r#""next-intl""#);
    }

    #[test]
    fn test_framework_deserialization_kebab_case() {
        let fw: Framework = serde_json::from_str(r#""react-i18next""#).unwrap();
        assert_eq!(fw, Framework::ReactI18next);

        let fw: Framework = serde_json::from_str(r#""next-intl""#).unwrap();
        assert_eq!(fw, Framework::NextIntl);
    }

    #[test]
    fn test_framework_default_is_next_intl() {
        assert_eq!(Framework::default(), Framework::NextIntl);
    }

    #[test]
    fn test_config_for_framework_react_i18next() {
        let config = Config::for_framework(Framework::ReactI18next);
        assert_eq!(config.framework, Framework::ReactI18next);
        assert_eq!(config.includes, vec!["src/components"]);
        assert_eq!(config.messages_root, "./src/locales");
    }

    #[test]
    fn test_config_for_framework_next_intl() {
        let config = Config::for_framework(Framework::NextIntl);
        assert_eq!(config.framework, Framework::NextIntl);
        assert!(config.includes.iter().any(|i| i.contains("app/[locale]")));
        assert_eq!(config.messages_root, "./messages");
    }

    #[test]
    fn test_no_framework_field_defaults_to_next_intl() {
        // Backward compatibility: existing configs without framework field
        // should behave as next-intl (the original default)
        let json = r#"{ "ignores": [] }"#;
        let raw: RawConfig = serde_json::from_str(json).unwrap();
        let config = raw.into_config();
        assert_eq!(config.framework, Framework::NextIntl);
        assert!(config.includes.iter().any(|i| i.contains("app/[locale]")));
        assert_eq!(config.messages_root, "./messages");
    }

    #[test]
    fn test_framework_next_intl_sets_correct_defaults() {
        let json = r#"{ "framework": "next-intl" }"#;
        let raw: RawConfig = serde_json::from_str(json).unwrap();
        let config = raw.into_config();
        assert_eq!(config.framework, Framework::NextIntl);
        assert!(config.includes.iter().any(|i| i.contains("app/[locale]")));
        assert_eq!(config.messages_root, "./messages");
    }

    #[test]
    fn test_framework_explicit_override_preserved() {
        // Even with next-intl framework, explicit messagesRoot should be kept
        let json = r#"{ "framework": "next-intl", "messagesRoot": "./custom" }"#;
        let raw: RawConfig = serde_json::from_str(json).unwrap();
        let config = raw.into_config();
        assert_eq!(config.framework, Framework::NextIntl);
        assert_eq!(config.messages_root, "./custom");
    }

    #[test]
    fn test_load_config_with_framework_react_i18next() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".glotrc.json");
        fs::write(&config_path, r#"{ "framework": "react-i18next" }"#).unwrap();

        let result = load_config(dir.path()).unwrap();
        assert!(result.from_file);
        assert_eq!(result.config.framework, Framework::ReactI18next);
        assert_eq!(result.config.includes, vec!["src/components"]);
        assert_eq!(result.config.messages_root, "./src/locales");
    }
}
