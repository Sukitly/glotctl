use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use colored::Colorize;
use glob::{Pattern, glob};
use walkdir::WalkDir;

use crate::config::TEST_FILE_PATTERNS;

/// Check if a pattern contains glob wildcards (* or ?).
/// Patterns without wildcards are treated as literal directory paths.
fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

/// Result of scanning files.
pub struct ScanResult {
    pub files: HashSet<String>,
    pub skipped_count: usize,
}

pub fn scan_files(
    base_dir: &str,
    includes: &[String],
    ignore_patterns: &[String],
    ignore_test_files: bool,
    verbose: bool,
) -> ScanResult {
    let mut files: HashSet<String> = HashSet::new();
    let mut skipped_count = 0;

    // Separate ignore patterns into literal paths and glob patterns
    let mut literal_ignore_paths: Vec<PathBuf> = Vec::new();
    let mut glob_patterns: Vec<Pattern> = Vec::new();

    // Process user-defined ignore patterns
    for p in ignore_patterns {
        if is_glob_pattern(p) {
            match Pattern::new(p) {
                Ok(pattern) => glob_patterns.push(pattern),
                Err(e) => {
                    if verbose {
                        eprintln!(
                            "{} Invalid ignore pattern '{}': {}",
                            "warning:".bold().yellow(),
                            p,
                            e
                        );
                    }
                }
            }
        } else {
            // Literal path mode: convert to absolute path for prefix matching
            let path = Path::new(base_dir).join(p);
            literal_ignore_paths.push(path);
        }
    }

    // Add test file patterns (these are always glob patterns)
    if ignore_test_files {
        for p in TEST_FILE_PATTERNS {
            if let Ok(pattern) = Pattern::new(p) {
                glob_patterns.push(pattern);
            }
        }
    }

    let dirs_to_scan: Vec<PathBuf> = if includes.is_empty() {
        vec![Path::new(base_dir).to_path_buf()]
    } else {
        let mut paths = Vec::new();
        for inc in includes {
            if is_glob_pattern(inc) {
                // Glob mode: expand pattern to matching directories
                let full_pattern = Path::new(base_dir).join(inc);
                let pattern_str = full_pattern.to_string_lossy();
                match glob(&pattern_str) {
                    Ok(entries) => {
                        for entry in entries.flatten() {
                            if entry.is_dir() {
                                paths.push(entry);
                            }
                        }
                    }
                    Err(e) => {
                        if verbose {
                            eprintln!(
                                "{} Invalid glob pattern '{}': {}",
                                "warning:".bold().yellow(),
                                inc,
                                e
                            );
                        }
                    }
                }
            } else {
                // Literal path mode: use as-is
                let path = Path::new(base_dir).join(inc);
                if path.exists() {
                    paths.push(path);
                } else if verbose {
                    eprintln!(
                        "{} Include path does not exist: {}",
                        "warning:".bold().yellow(),
                        path.display()
                    );
                }
            }
        }
        paths
    };

    for dir in dirs_to_scan {
        for entry in WalkDir::new(dir) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    skipped_count += 1;
                    if verbose {
                        eprintln!("{} Cannot access path: {}", "warning:".bold().yellow(), e);
                    }
                    continue;
                }
            };
            let path = entry.path();
            let path_str = path.to_string_lossy();

            // Check if path matches any literal ignore path (prefix match)
            if literal_ignore_paths
                .iter()
                .any(|ignore_path| path.starts_with(ignore_path))
            {
                continue;
            }

            // Check if path matches any glob pattern
            if glob_patterns.iter().any(|p| p.matches(&path_str)) {
                continue;
            }

            if path.is_file() && is_scannable_file(path) {
                files.insert(path_str.into());
            }
        }
    }

    ScanResult {
        files,
        skipped_count,
    }
}

fn is_scannable_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("tsx" | "ts" | "jsx" | "js")
    )
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_scan_tsx_files() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        File::create(dir_path.join("app.tsx")).unwrap();
        File::create(dir_path.join("utils.ts")).unwrap();
        File::create(dir_path.join("style.css")).unwrap();

        let result = scan_files(dir_path.to_str().unwrap(), &[], &[], false, false);

        assert_eq!(result.files.len(), 2);
        assert!(result.files.iter().any(|f| f.ends_with("app.tsx")));
        assert!(result.files.iter().any(|f| f.ends_with("utils.ts")));
        assert!(!result.files.iter().any(|f| f.ends_with("style.css")));
    }

    #[test]
    fn test_scan_ignores_node_modules() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let node_modules = dir_path.join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        File::create(node_modules.join("lib.ts")).unwrap();

        File::create(dir_path.join("app.tsx")).unwrap();

        let result = scan_files(
            dir_path.to_str().unwrap(),
            &[],
            &["**/node_modules/**".to_owned()],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
        assert!(result.files.iter().any(|f| f.ends_with("app.tsx")));
        assert!(!result.files.iter().any(|f| f.contains("node_modules")));
    }

    #[test]
    fn test_scan_nested_directories() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let components = dir_path.join("components");
        fs::create_dir(&components).unwrap();
        File::create(components.join("Button.tsx")).unwrap();

        let utils = dir_path.join("utils");
        fs::create_dir(&utils).unwrap();
        File::create(utils.join("helper.ts")).unwrap();

        let result = scan_files(dir_path.to_str().unwrap(), &[], &[], false, false);

        assert_eq!(result.files.len(), 2);
        assert!(
            result
                .files
                .iter()
                .any(|f| f.ends_with("components/Button.tsx"))
        );
        assert!(result.files.iter().any(|f| f.ends_with("utils/helper.ts")));
    }

    #[test]
    fn test_is_scannable_file() {
        assert!(is_scannable_file(Path::new("app.tsx")));
        assert!(is_scannable_file(Path::new("app.ts")));
        assert!(is_scannable_file(Path::new("app.jsx")));
        assert!(is_scannable_file(Path::new("app.js")));
        assert!(!is_scannable_file(Path::new("style.css")));
        assert!(!is_scannable_file(Path::new("data.json")));
        assert!(!is_scannable_file(Path::new("README.md")));
    }
    #[test]
    fn test_scan_with_includes() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let src = dir_path.join("src");
        fs::create_dir(&src).unwrap();
        File::create(src.join("app.tsx")).unwrap();

        let lib = dir_path.join("lib");
        fs::create_dir(&lib).unwrap();
        File::create(lib.join("utils.ts")).unwrap();

        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src".to_owned()],
            &[],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
        assert!(result.files.iter().any(|f| f.ends_with("src/app.tsx")));
    }

    #[test]
    fn test_scan_with_multiple_includes() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let src = dir_path.join("src");
        fs::create_dir(&src).unwrap();
        File::create(src.join("app.tsx")).unwrap();

        let app = dir_path.join("app");
        fs::create_dir(&app).unwrap();
        File::create(app.join("page.tsx")).unwrap();

        let lib = dir_path.join("lib");
        fs::create_dir(&lib).unwrap();
        File::create(lib.join("utils.ts")).unwrap();

        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src".to_owned(), "app".to_owned()],
            &[],
            false,
            false,
        );

        assert_eq!(result.files.len(), 2);
        assert!(result.files.iter().any(|f| f.ends_with("src/app.tsx")));
        assert!(result.files.iter().any(|f| f.ends_with("app/page.tsx")));
    }

    #[test]
    fn test_scan_with_nonexistent_include() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        let src = dir_path.join("src");
        fs::create_dir(&src).unwrap();
        File::create(src.join("app.tsx")).unwrap();

        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src".to_owned(), "nonexistent".to_owned()],
            &[],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn test_scan_ignores_test_files() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        File::create(dir_path.join("app.tsx")).unwrap();
        File::create(dir_path.join("app.test.tsx")).unwrap();
        File::create(dir_path.join("utils.spec.jsx")).unwrap();

        let tests_dir = dir_path.join("__tests__");
        fs::create_dir(&tests_dir).unwrap();
        File::create(tests_dir.join("helper.test.ts")).unwrap();

        let result = scan_files(dir_path.to_str().unwrap(), &[], &[], true, false);

        assert_eq!(result.files.len(), 1);
        assert!(result.files.iter().any(|f| f.ends_with("app.tsx")));
    }

    #[test]
    fn test_scan_includes_test_files_when_disabled() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        File::create(dir_path.join("app.tsx")).unwrap();
        File::create(dir_path.join("app.test.tsx")).unwrap();

        let result = scan_files(dir_path.to_str().unwrap(), &[], &[], false, false);

        assert_eq!(result.files.len(), 2);
    }

    #[test]
    fn test_scan_deduplicates_overlapping_includes() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create nested structure: src/components/Button.tsx
        let src = dir_path.join("src");
        fs::create_dir(&src).unwrap();
        let components = src.join("components");
        fs::create_dir(&components).unwrap();
        File::create(components.join("Button.tsx")).unwrap();

        // Include both "src" and "src/components" - overlapping paths
        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src".to_owned(), "src/components".to_owned()],
            &[],
            false,
            false,
        );

        // Button.tsx should only appear once, not twice
        assert_eq!(result.files.len(), 1);
        assert!(result.files.iter().any(|f| f.ends_with("Button.tsx")));
    }

    #[test]
    fn test_scan_with_glob_pattern() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create: src/app/page.tsx, src/components/Button.tsx, lib/utils.ts
        let src_app = dir_path.join("src").join("app");
        fs::create_dir_all(&src_app).unwrap();
        File::create(src_app.join("page.tsx")).unwrap();

        let src_components = dir_path.join("src").join("components");
        fs::create_dir_all(&src_components).unwrap();
        File::create(src_components.join("Button.tsx")).unwrap();

        let lib = dir_path.join("lib");
        fs::create_dir(&lib).unwrap();
        File::create(lib.join("utils.ts")).unwrap();

        // Use glob pattern to match directories under src/
        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src/*".to_owned()],
            &[],
            false,
            false,
        );

        assert_eq!(result.files.len(), 2);
        assert!(result.files.iter().any(|f| f.ends_with("page.tsx")));
        assert!(result.files.iter().any(|f| f.ends_with("Button.tsx")));
        // lib/utils.ts should not be included
        assert!(!result.files.iter().any(|f| f.ends_with("utils.ts")));
    }

    #[test]
    fn test_scan_with_literal_bracket_path() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create: app/[locale]/page.tsx (Next.js dynamic route)
        let locale_dir = dir_path.join("app").join("[locale]");
        fs::create_dir_all(&locale_dir).unwrap();
        File::create(locale_dir.join("page.tsx")).unwrap();

        // Also create app/other/page.tsx
        let other_dir = dir_path.join("app").join("other");
        fs::create_dir_all(&other_dir).unwrap();
        File::create(other_dir.join("other.tsx")).unwrap();

        // Use literal path (no * or ?), [locale] should be treated literally
        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["app/[locale]".to_owned()],
            &[],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
        assert!(
            result
                .files
                .iter()
                .any(|f| f.ends_with("[locale]/page.tsx"))
        );
        // app/other/other.tsx should not be included
        assert!(!result.files.iter().any(|f| f.ends_with("other.tsx")));
    }

    #[test]
    fn test_is_glob_pattern() {
        assert!(is_glob_pattern("src/*"));
        assert!(is_glob_pattern("src/**/*.tsx"));
        assert!(is_glob_pattern("file?.ts"));
        assert!(!is_glob_pattern("src"));
        assert!(!is_glob_pattern("app/[locale]")); // [locale] without * or ? is literal
        assert!(!is_glob_pattern("src/components"));
    }

    #[test]
    fn test_scan_ignores_literal_directory_path() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create: src/components/Button.tsx, src/components/ai-elements/Chat.tsx
        let components = dir_path.join("src").join("components");
        fs::create_dir_all(&components).unwrap();
        File::create(components.join("Button.tsx")).unwrap();

        let ai_elements = components.join("ai-elements");
        fs::create_dir_all(&ai_elements).unwrap();
        File::create(ai_elements.join("Chat.tsx")).unwrap();

        // Use literal path to ignore ai-elements directory
        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src".to_owned()],
            &["src/components/ai-elements".to_owned()],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
        assert!(result.files.iter().any(|f| f.ends_with("Button.tsx")));
        assert!(!result.files.iter().any(|f| f.contains("ai-elements")));
    }

    #[test]
    fn test_scan_ignores_mixed_patterns() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create directory structure:
        // src/components/Button.tsx
        // src/components/Button.stories.tsx
        // src/generated/types.ts
        let components = dir_path.join("src").join("components");
        fs::create_dir_all(&components).unwrap();
        File::create(components.join("Button.tsx")).unwrap();
        File::create(components.join("Button.stories.tsx")).unwrap();

        let generated = dir_path.join("src").join("generated");
        fs::create_dir_all(&generated).unwrap();
        File::create(generated.join("types.ts")).unwrap();

        // Mix literal path and glob pattern
        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["src".to_owned()],
            &[
                "src/generated".to_owned(),    // literal path
                "**/*.stories.tsx".to_owned(), // glob pattern
            ],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
        assert!(result.files.iter().any(|f| f.ends_with("Button.tsx")));
        assert!(!result.files.iter().any(|f| f.contains("generated")));
        assert!(!result.files.iter().any(|f| f.contains("stories")));
    }

    #[test]
    fn test_scan_ignores_nested_literal_path() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create: app/[locale]/page.tsx, app/[locale]/admin/page.tsx
        let locale_dir = dir_path.join("app").join("[locale]");
        fs::create_dir_all(&locale_dir).unwrap();
        File::create(locale_dir.join("page.tsx")).unwrap();

        let admin_dir = locale_dir.join("admin");
        fs::create_dir_all(&admin_dir).unwrap();
        File::create(admin_dir.join("page.tsx")).unwrap();

        // Ignore the admin directory using literal path with [locale]
        let result = scan_files(
            dir_path.to_str().unwrap(),
            &["app/[locale]".to_owned()],
            &["app/[locale]/admin".to_owned()],
            false,
            false,
        );

        assert_eq!(result.files.len(), 1);
        assert!(
            result
                .files
                .iter()
                .any(|f| f.ends_with("[locale]/page.tsx"))
        );
        assert!(!result.files.iter().any(|f| f.contains("admin")));
    }
}
