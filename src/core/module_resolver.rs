//! TypeScript and JavaScript module path resolution.
//!
//! Supports:
//! - Relative imports (`./foo`, `../bar`)
//! - `tsconfig.json` / `jsconfig.json` `paths` aliases
//! - `baseUrl`-relative imports
//! - `extends` chains in project configs
//!
//! The resolver is intentionally conservative for non-relative imports:
//! - `paths` matches may return speculative local paths for cross-file registry lookups
//! - plain `baseUrl` imports only resolve when the target file actually exists
//! - package imports continue to return `None`

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use regex::Regex;
use serde::Deserialize;

const PROJECT_CONFIG_FILES: &[&str] = &["tsconfig.json", "jsconfig.json"];
const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

#[derive(Debug, Default)]
struct ResolverCache {
    nearest_project_config: HashMap<PathBuf, Option<PathBuf>>,
    project_configs: HashMap<PathBuf, Option<ProjectConfig>>,
}

fn resolver_cache() -> &'static Mutex<ResolverCache> {
    static CACHE: OnceLock<Mutex<ResolverCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ResolverCache::default()))
}

#[derive(Debug, Clone, Default)]
struct ProjectConfig {
    base_url: Option<PathBuf>,
    path_mappings: Vec<PathMapping>,
}

impl ProjectConfig {
    fn apply_paths(&mut self, paths: HashMap<String, Vec<String>>, target_base_dir: &Path) {
        let overridden_patterns: HashSet<_> = paths.keys().cloned().collect();
        self.path_mappings
            .retain(|mapping| !overridden_patterns.contains(&mapping.pattern));

        for (pattern, targets) in paths {
            if let Some(mapping) = PathMapping::new(&pattern, targets, target_base_dir) {
                self.path_mappings.push(mapping);
            }
        }

        self.path_mappings.sort_by(|left, right| {
            right
                .specificity
                .cmp(&left.specificity)
                .then_with(|| left.pattern.cmp(&right.pattern))
        });
    }

    fn resolve_non_relative_import(&self, import_path: &str) -> Option<String> {
        let mut speculative_match = None;

        for mapping in &self.path_mappings {
            let Some(captures) = mapping.capture_segments(import_path) else {
                continue;
            };

            for target_pattern in &mapping.target_patterns {
                let candidate = mapping
                    .target_base_dir
                    .join(substitute_path_pattern(target_pattern, &captures));

                if let Some(resolved) = resolve_existing_candidate(&candidate) {
                    return Some(resolved);
                }

                if speculative_match.is_none() {
                    speculative_match = Some(speculative_candidate(&candidate));
                }
            }
        }

        if speculative_match.is_some() {
            return speculative_match;
        }

        self.base_url
            .as_ref()
            .and_then(|base_url| resolve_existing_candidate(&base_url.join(import_path)))
    }
}

#[derive(Debug, Clone)]
struct PathMapping {
    pattern: String,
    matcher: Regex,
    target_patterns: Vec<String>,
    target_base_dir: PathBuf,
    specificity: usize,
}

impl PathMapping {
    fn new(pattern: &str, target_patterns: Vec<String>, target_base_dir: &Path) -> Option<Self> {
        if target_patterns.is_empty() {
            return None;
        }

        let matcher = compile_path_pattern(pattern)?;

        Some(Self {
            pattern: pattern.to_string(),
            matcher,
            target_patterns,
            target_base_dir: normalize_path(target_base_dir.to_path_buf()),
            specificity: pattern.chars().filter(|ch| *ch != '*').count(),
        })
    }

    fn capture_segments(&self, import_path: &str) -> Option<Vec<String>> {
        let captures = self.matcher.captures(import_path)?;
        let mut segments = Vec::new();

        for idx in 1..captures.len() {
            segments.push(
                captures
                    .get(idx)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
            );
        }

        Some(segments)
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawProjectConfig {
    #[serde(default)]
    extends: Option<String>,
    #[serde(default, rename = "compilerOptions")]
    compiler_options: RawCompilerOptions,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawCompilerOptions {
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    paths: HashMap<String, Vec<String>>,
}

/// Resolve an import path to a source file path.
pub fn resolve_import_path(current_file: &Path, import_path: &str) -> Option<String> {
    if import_path.starts_with('.') {
        return resolve_relative_import(current_file, import_path);
    }

    let current_dir = current_file.parent().unwrap_or_else(|| Path::new(""));
    let project_config_path = find_nearest_project_config(current_dir)?;
    let project_config = load_project_config(&project_config_path)?;
    project_config.resolve_non_relative_import(import_path)
}

fn resolve_relative_import(current_file: &Path, import_path: &str) -> Option<String> {
    let base_dir = current_file.parent().unwrap_or_else(|| Path::new(""));
    let normalized_import = import_path.strip_prefix("./").unwrap_or(import_path);
    let candidate = base_dir.join(normalized_import);
    resolve_existing_or_speculative_candidate(&candidate, true)
}

fn find_nearest_project_config(start_dir: &Path) -> Option<PathBuf> {
    let normalized_start = normalize_path(start_dir.to_path_buf());

    if let Some(cached) = resolver_cache()
        .lock()
        .unwrap()
        .nearest_project_config
        .get(&normalized_start)
        .cloned()
    {
        return cached;
    }

    let mut current = normalized_start.clone();
    let mut resolved = None;

    loop {
        for config_name in PROJECT_CONFIG_FILES {
            let candidate = current.join(config_name);
            if candidate.is_file() {
                resolved = Some(candidate);
                break;
            }
        }

        if resolved.is_some() || !current.pop() {
            break;
        }
    }

    resolver_cache()
        .lock()
        .unwrap()
        .nearest_project_config
        .insert(normalized_start, resolved.clone());

    resolved
}

fn load_project_config(config_path: &Path) -> Option<ProjectConfig> {
    let normalized_path = normalize_path(config_path.to_path_buf());

    if let Some(cached) = resolver_cache()
        .lock()
        .unwrap()
        .project_configs
        .get(&normalized_path)
        .cloned()
    {
        return cached;
    }

    let parsed = parse_project_config(&normalized_path);

    resolver_cache()
        .lock()
        .unwrap()
        .project_configs
        .insert(normalized_path, parsed.clone());

    parsed
}

fn parse_project_config(config_path: &Path) -> Option<ProjectConfig> {
    let raw = read_project_config(config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new(""));

    let mut project_config = raw
        .extends
        .as_deref()
        .and_then(|extends| resolve_extends_path(config_dir, extends))
        .and_then(|path| load_project_config(&path))
        .unwrap_or_default();

    if let Some(base_url) = raw.compiler_options.base_url.as_deref() {
        project_config.base_url = Some(normalize_path(config_dir.join(base_url)));
    }

    if !raw.compiler_options.paths.is_empty() {
        let target_base_dir = project_config
            .base_url
            .clone()
            .unwrap_or_else(|| normalize_path(config_dir.to_path_buf()));
        project_config.apply_paths(raw.compiler_options.paths, &target_base_dir);
    }

    Some(project_config)
}

fn read_project_config(config_path: &Path) -> Option<RawProjectConfig> {
    let content = fs::read_to_string(config_path).ok()?;
    let without_comments = strip_json_comments(content.trim_start_matches('\u{feff}'));
    let sanitized = strip_trailing_commas(&without_comments);
    serde_json::from_str(&sanitized).ok()
}

fn resolve_extends_path(config_dir: &Path, extends: &str) -> Option<PathBuf> {
    let extends_path = Path::new(extends);

    if extends_path.is_absolute() || extends.starts_with('.') {
        return resolve_extends_candidate(config_dir.join(extends_path));
    }

    let mut current = Some(config_dir);
    while let Some(dir) = current {
        let node_modules_base = dir.join("node_modules").join(extends);
        if let Some(resolved) = resolve_extends_candidate(node_modules_base) {
            return Some(resolved);
        }
        current = dir.parent();
    }

    None
}

fn resolve_extends_candidate(base: PathBuf) -> Option<PathBuf> {
    let mut candidates = vec![base.clone()];

    if base.extension().is_none() {
        candidates.push(base.with_extension("json"));
    }

    candidates.push(base.join("tsconfig.json"));
    candidates.push(base.join("jsconfig.json"));

    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn compile_path_pattern(pattern: &str) -> Option<Regex> {
    let escaped = regex::escape(pattern).replace("\\*", "(.*)");
    Regex::new(&format!("^{escaped}$")).ok()
}

fn substitute_path_pattern(pattern: &str, captures: &[String]) -> String {
    let mut result = String::new();

    for (idx, segment) in pattern.split('*').enumerate() {
        if idx > 0
            && let Some(capture) = captures.get(idx - 1)
        {
            result.push_str(capture);
        }
        result.push_str(segment);
    }

    result
}

fn resolve_existing_or_speculative_candidate(
    candidate: &Path,
    allow_speculative: bool,
) -> Option<String> {
    resolve_existing_candidate(candidate).or_else(|| {
        if allow_speculative {
            Some(speculative_candidate(candidate))
        } else {
            None
        }
    })
}

fn resolve_existing_candidate(candidate: &Path) -> Option<String> {
    let normalized_candidate = normalize_path(candidate.to_path_buf());

    if normalized_candidate.is_file() {
        return Some(normalized_candidate.to_string_lossy().to_string());
    }

    if normalized_candidate.extension().is_none() {
        for ext in SOURCE_EXTENSIONS {
            let with_ext = normalized_candidate.with_extension(ext);
            if with_ext.is_file() {
                return Some(with_ext.to_string_lossy().to_string());
            }
        }
    }

    for ext in SOURCE_EXTENSIONS {
        let index_path = normalized_candidate.join(format!("index.{ext}"));
        if index_path.is_file() {
            return Some(index_path.to_string_lossy().to_string());
        }
    }

    None
}

fn speculative_candidate(candidate: &Path) -> String {
    let speculative = if candidate.extension().is_some() {
        candidate.to_path_buf()
    } else {
        candidate.with_extension("ts")
    };

    normalize_path(speculative).to_string_lossy().to_string()
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let had_leading_cur_dir = matches!(path.components().next(), Some(Component::CurDir));
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    let starts_with_parent = matches!(normalized.components().next(), Some(Component::ParentDir));
    if had_leading_cur_dir && !normalized.is_absolute() && !starts_with_parent {
        if normalized.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(".").join(normalized)
        }
    } else {
        normalized
    }
}

fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                result.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            } else if ch == '\n' {
                result.push('\n');
            }
            continue;
        }

        if in_string {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            result.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    in_line_comment = true;
                    continue;
                }
                Some('*') => {
                    chars.next();
                    in_block_comment = true;
                    continue;
                }
                _ => {}
            }
        }

        result.push(ch);
    }

    result
}

fn strip_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;
    let mut in_string = false;
    let mut escaped = false;

    while idx < chars.len() {
        let ch = chars[idx];

        if in_string {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            idx += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            result.push(ch);
            idx += 1;
            continue;
        }

        if ch == ',' {
            let mut next = idx + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }

            if next < chars.len() && matches!(chars[next], '}' | ']') {
                idx += 1;
                continue;
            }
        }

        result.push(ch);
        idx += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{normalize_path, resolve_import_path};

    #[test]
    fn test_resolve_relative_import() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("utils.ts"), "export const value = 1;").unwrap();
        fs::write(src_dir.join("app.tsx"), "export default null;").unwrap();

        let resolved = resolve_import_path(&src_dir.join("app.tsx"), "./utils");

        assert_eq!(
            resolved,
            Some(
                normalize_path(src_dir.join("utils.ts"))
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    #[test]
    fn test_resolve_paths_alias_without_base_url() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src").join("lib");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("schema.ts"), "export const value = 1;").unwrap();
        fs::write(
            dir.path().join("src").join("app.tsx"),
            "export default null;",
        )
        .unwrap();

        fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{
  "compilerOptions": {
    "strict": true,
  },
}
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{
  // Alias config
  "extends": "./tsconfig.base",
  "compilerOptions": {
    "paths": {
      "@/*": ["./src/*"],
    },
  },
}
"#,
        )
        .unwrap();

        let resolved = resolve_import_path(&dir.path().join("src/app.tsx"), "@/lib/schema");

        assert_eq!(
            resolved,
            Some(
                normalize_path(dir.path().join("src/lib/schema.ts"))
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    #[test]
    fn test_resolve_base_url_import() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src").join("lib");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("helpers.ts"), "export const value = 1;").unwrap();
        fs::write(
            dir.path().join("src").join("app.tsx"),
            "export default null;",
        )
        .unwrap();

        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{
  "compilerOptions": {
    "baseUrl": "./src"
  }
}
"#,
        )
        .unwrap();

        let resolved = resolve_import_path(&dir.path().join("src/app.tsx"), "lib/helpers");

        assert_eq!(
            resolved,
            Some(
                normalize_path(dir.path().join("src/lib/helpers.ts"))
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    #[test]
    fn test_resolve_jsconfig_when_tsconfig_is_missing() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src").join("messages");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("index.ts"), "export const value = 1;").unwrap();
        fs::write(
            dir.path().join("src").join("app.tsx"),
            "export default null;",
        )
        .unwrap();

        fs::write(
            dir.path().join("jsconfig.json"),
            r#"{
  "compilerOptions": {
    "paths": {
      "@messages": ["./src/messages"]
    }
  }
}
"#,
        )
        .unwrap();

        let resolved = resolve_import_path(&dir.path().join("src/app.tsx"), "@messages");

        assert_eq!(
            resolved,
            Some(
                normalize_path(dir.path().join("src/messages/index.ts"))
                    .to_string_lossy()
                    .to_string()
            )
        );
    }
}
