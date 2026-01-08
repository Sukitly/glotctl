use std::{cmp::Ordering, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    HardcodedText,
    MissingKey,
    DynamicKey,
    ReplicaLag,
    UnusedKey,
    OrphanKey,
    UntrackedNamespace,
    ParseError,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rule::HardcodedText => write!(f, "hardcoded-text"),
            Rule::MissingKey => write!(f, "missing-key"),
            Rule::DynamicKey => write!(f, "dynamic-key"),
            Rule::ReplicaLag => write!(f, "replica-lag"),
            Rule::UnusedKey => write!(f, "unused-key"),
            Rule::OrphanKey => write!(f, "orphan-key"),
            Rule::UntrackedNamespace => write!(f, "untracked-namespace"),
            Rule::ParseError => write!(f, "parse-error"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub file_path: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub message: String,
    pub severity: Severity,
    pub rule: Rule,
    pub details: Option<String>,
    pub source_line: Option<String>,
    pub hint: Option<String>,
}

impl Issue {
    pub fn hardcoded(
        file_path: &str,
        line: usize,
        col: usize,
        text: &str,
        source_line: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: text.to_string(),
            severity: Severity::Error,
            rule: Rule::HardcodedText,
            details: None,
            source_line,
            hint: None,
        }
    }

    pub fn missing_key(
        file_path: &str,
        line: usize,
        col: usize,
        key: &str,
        source_line: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: key.to_string(),
            severity: Severity::Error,
            rule: Rule::MissingKey,
            details: None,
            source_line,
            hint: None,
        }
    }

    pub fn missing_key_from_schema(
        file_path: &str,
        line: usize,
        col: usize,
        key: &str,
        schema_name: &str,
        schema_file: &str,
        source_line: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: key.to_string(),
            severity: Severity::Error,
            rule: Rule::MissingKey,
            details: Some(format!("from {} ({})", schema_name, schema_file)),
            source_line,
            hint: None,
        }
    }

    pub fn dynamic_key(
        file_path: &str,
        line: usize,
        col: usize,
        reason: &str,
        source_line: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: reason.to_string(),
            severity: Severity::Warning,
            rule: Rule::DynamicKey,
            details: None,
            source_line,
            hint: None,
        }
    }

    pub fn dynamic_key_with_hint(
        file_path: &str,
        line: usize,
        col: usize,
        reason: &str,
        source_line: Option<String>,
        hint: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: reason.to_string(),
            severity: Severity::Warning,
            rule: Rule::DynamicKey,
            details: None,
            source_line,
            hint,
        }
    }

    pub fn replica_lag(
        key: &str,
        value: &str,
        file_path: &str,
        line: usize,
        missing_in: &[String],
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: None,
            message: key.to_string(),
            severity: Severity::Error,
            rule: Rule::ReplicaLag,
            details: Some(format!(
                "(\"{}\") missing in: {}",
                value,
                missing_in.join(", ")
            )),
            source_line: None,
            hint: None,
        }
    }

    pub fn unused_key(key: &str, value: &str, file_path: &str, line: usize) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: None,
            message: key.to_string(),
            severity: Severity::Warning,
            rule: Rule::UnusedKey,
            details: Some(format!("(\"{}\")", value)),
            source_line: None,
            hint: None,
        }
    }

    pub fn orphan_key(key: &str, locale: &str, value: &str, file_path: &str, line: usize) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: None,
            message: key.to_string(),
            severity: Severity::Warning,
            rule: Rule::OrphanKey,
            details: Some(format!("in {} (\"{}\")", locale, value)),
            source_line: None,
            hint: None,
        }
    }

    pub fn untracked_namespace(
        file_path: &str,
        line: usize,
        col: usize,
        schema_name: &str,
        raw_key: &str,
        source_line: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: raw_key.to_string(),
            severity: Severity::Warning,
            rule: Rule::UntrackedNamespace,
            details: Some(format!(
                "from {} - namespace could not be determined",
                schema_name
            )),
            source_line,
            hint: None,
        }
    }

    pub fn missing_dynamic_key_candidates(
        file_path: &str,
        line: usize,
        col: usize,
        source_object: &str,
        missing_keys: &[String],
        source_line: Option<String>,
    ) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(line),
            col: Some(col),
            message: format!("dynamic key from \"{}\"", source_object),
            severity: Severity::Error,
            rule: Rule::MissingKey,
            details: Some(format!("missing: {}", missing_keys.join(", "))),
            source_line,
            hint: None,
        }
    }

    pub fn parse_error(file_path: &str, error: &str) -> Self {
        Self {
            file_path: Some(file_path.to_string()),
            line: Some(1),
            col: Some(1),
            message: format!("Failed to parse: {}", error),
            severity: Severity::Error,
            rule: Rule::ParseError,
            details: None,
            source_line: None,
            hint: None,
        }
    }
}

impl Ord for Issue {
    fn cmp(&self, other: &Self) -> Ordering {
        // Sort by: file_path (None last), line, col, message
        //
        // Note: message comparison is needed for deterministic ordering because:
        // - HashMap iteration order is non-deterministic
        // - Multiple issues can have same file_path/line/col (e.g., unused keys in same JSON file)
        // - Without message comparison, test output would be flaky
        match (&self.file_path, &other.file_path) {
            (Some(a), Some(b)) => a
                .cmp(b)
                .then_with(|| self.line.cmp(&other.line))
                .then_with(|| self.col.cmp(&other.col))
                .then_with(|| self.message.cmp(&other.message)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => self
                .rule
                .cmp(&other.rule)
                .then_with(|| self.message.cmp(&other.message)),
        }
    }
}

impl PartialOrd for Issue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
