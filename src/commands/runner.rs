use std::collections::HashSet;

use anyhow::{Result, bail};
use clap::ValueEnum;

use crate::{
    RunResult,
    args::CheckArgs,
    commands::context::CheckContext,
    issue::{Issue, IssueReport, Rule, Severity},
    rules,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum CheckType {
    Hardcoded,
    Missing,
    Orphan,
    Untranslated,
    TypeMismatch,
}

/// The main orchestrator for check operations.
///
/// CheckRunner is responsible for:
/// 1. Holding the CheckContext (data container)
/// 2. Managing which checks to run
/// 3. Lazy-loading data via CheckContext based on check requirements
/// 4. Calling pure rule functions and aggregating issues
pub struct CheckRunner {
    checks: HashSet<CheckType>,
    ctx: CheckContext,
}

impl CheckRunner {
    pub fn new(args: CheckArgs) -> Result<Self> {
        let ctx = CheckContext::new(&args.common)?;
        Ok(Self {
            checks: HashSet::new(),
            ctx,
        })
    }

    pub fn add(mut self, check: CheckType) -> Self {
        self.checks.insert(check);
        self
    }

    pub fn hardcoded(self) -> Self {
        self.add(CheckType::Hardcoded)
    }

    pub fn missing(self) -> Self {
        self.add(CheckType::Missing)
    }

    pub fn orphan(self) -> Self {
        self.add(CheckType::Orphan)
    }

    pub fn untranslated(self) -> Self {
        self.add(CheckType::Untranslated)
    }

    pub fn type_mismatch(self) -> Self {
        self.add(CheckType::TypeMismatch)
    }

    pub fn all(self) -> Self {
        self.hardcoded()
            .missing()
            .orphan()
            .untranslated()
            .type_mismatch()
    }

    pub fn run(self) -> Result<RunResult> {
        let mut all_issues = Vec::new();

        // Determine what data we need
        let needs_extractions = self.checks.contains(&CheckType::Missing)
            || self.checks.contains(&CheckType::Untranslated)
            || self.checks.contains(&CheckType::TypeMismatch)
            || self.checks.contains(&CheckType::Orphan);
        let needs_messages = self.checks.contains(&CheckType::Missing)
            || self.checks.contains(&CheckType::Orphan)
            || self.checks.contains(&CheckType::Untranslated)
            || self.checks.contains(&CheckType::TypeMismatch);

        // 1. Parse files first - all checkers need parsed ASTs
        let mut parse_errors = self.ctx.ensure_parsed_files();
        all_issues.append(&mut parse_errors);

        // 2. Load registries if needed (for extractions)
        if needs_extractions {
            self.ctx.ensure_registries()?;
        }

        // 3. Load messages if needed
        if needs_messages {
            self.ctx.ensure_messages()?;

            let messages = self.ctx.messages().expect("messages should be loaded");
            if messages.primary_messages.is_none() {
                let available: Vec<_> = messages.all_messages.keys().collect();
                let hint = if available.is_empty() {
                    "No locale files found in the directory.".to_string()
                } else {
                    format!("Available locales: {:?}", available)
                };
                bail!(
                    "Primary locale file '{}.json' not found in '{}'.\n\
                     {}\n\
                     Hint: Check your .glotrc.json 'primaryLocale' setting.",
                    self.ctx.config.primary_locale,
                    self.ctx.resolved_messages_dir().display(),
                    hint
                );
            }
        }

        // 4. Load extractions if needed
        if needs_extractions {
            self.ctx.ensure_extractions()?;
        }

        // 5. Run hardcoded check
        if self.checks.contains(&CheckType::Hardcoded) {
            if self.ctx.verbose {
                eprintln!("Running check: hardcoded");
            }
            self.ctx.ensure_hardcoded_issues()?;
            let hardcoded_issues = self
                .ctx
                .hardcoded_issues()
                .expect("hardcoded_issues must be loaded");

            let issues = rules::check_hardcoded(hardcoded_issues);
            all_issues.extend(issues.into_iter().map(|i| {
                // Convert new HardcodedIssue to old Issue type
                Issue::Hardcoded(crate::issue::HardcodedIssue {
                    location: crate::issue::SourceLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    )
                    .with_col(i.context.location.col)
                    .with_jsx_context(i.context.comment_style.is_jsx()),
                    text: i.text,
                    source_line: Some(i.context.source_line),
                })
            }));
        }

        // 6. Run missing/unresolved/replica-lag checks
        if self.checks.contains(&CheckType::Missing) {
            if self.ctx.verbose {
                eprintln!("Running check: missing");
            }
            let extractions = self.ctx.all_key_usages().expect("extractions must be loaded");
            let messages = self.ctx.messages().expect("messages must be loaded");
            let registries = self.ctx.registries().expect("registries must be loaded");
            let primary_messages = messages
                .primary_messages
                .as_ref()
                .expect("primary_messages must exist");

            // Check missing keys
            let missing_issues =
                rules::check_missing_key(extractions, primary_messages, registries);
            all_issues.extend(missing_issues.into_iter().map(|i| {
                Issue::MissingKey(crate::issue::MissingKeyIssue {
                    location: crate::issue::SourceLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    )
                    .with_col(i.context.location.col),
                    key: i.key,
                    source_line: Some(i.context.source_line),
                    from_schema: i.from_schema,
                })
            }));

            // Check unresolved keys
            let unresolved_issues = rules::check_unresolved_key(extractions);
            all_issues.extend(unresolved_issues.into_iter().map(|i| {
                Issue::UnresolvedKey(crate::issue::UnresolvedKeyIssue {
                    location: crate::issue::SourceLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    )
                    .with_col(i.context.location.col),
                    reason: convert_unresolved_reason(&i.reason),
                    source_line: Some(i.context.source_line),
                    hint: i.hint,
                    pattern: i.pattern,
                })
            }));

            // Check replica lag
            let key_usages = rules::build_key_usage_map(extractions);
            let replica_lag_issues = rules::check_replica_lag(
                &self.ctx.config.primary_locale,
                &messages.all_messages,
                &key_usages,
            );
            all_issues.extend(replica_lag_issues.into_iter().map(|i| {
                let usages = i
                    .usages
                    .iter()
                    .map(|u| {
                        crate::issue::KeyUsage::new(
                            crate::issue::SourceLocation::new(
                                &u.location.file_path,
                                u.location.line,
                            )
                            .with_col(u.location.col)
                            .with_jsx_context(u.comment_style.is_jsx()),
                        )
                    })
                    .collect::<Vec<_>>();
                let total_usages = usages.len();
                Issue::ReplicaLag(crate::issue::ReplicaLagIssue {
                    location: crate::issue::MessageLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    ),
                    key: i.context.key,
                    value: i.context.value,
                    primary_locale: i.primary_locale,
                    missing_in: i.missing_in,
                    usages,
                    total_usages,
                })
            }));
        }

        // 7. Run orphan/unused checks
        if self.checks.contains(&CheckType::Orphan) {
            if self.ctx.verbose {
                eprintln!("Running check: orphan");
            }
            self.ctx.ensure_used_keys()?;
            let used_keys = self.ctx.used_keys().expect("used_keys must be loaded");
            let messages = self.ctx.messages().expect("messages must be loaded");
            let primary_messages = messages
                .primary_messages
                .as_ref()
                .expect("primary_messages must exist");

            // Check unused keys
            let unused_issues = rules::check_unused_key(used_keys, primary_messages);
            all_issues.extend(unused_issues.into_iter().map(|i| {
                Issue::UnusedKey(crate::issue::UnusedKeyIssue {
                    location: crate::issue::MessageLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    ),
                    key: i.context.key,
                    value: i.context.value,
                })
            }));

            // Check orphan keys
            let orphan_issues =
                rules::check_orphan_key(&self.ctx.config.primary_locale, &messages.all_messages);
            all_issues.extend(orphan_issues.into_iter().map(|i| {
                Issue::OrphanKey(crate::issue::OrphanKeyIssue {
                    location: crate::issue::MessageLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    ),
                    key: i.context.key,
                    value: i.context.value,
                    locale: i.locale,
                })
            }));
        }

        // 8. Run untranslated check
        if self.checks.contains(&CheckType::Untranslated) {
            if self.ctx.verbose {
                eprintln!("Running check: untranslated");
            }
            let extractions = self.ctx.all_key_usages().expect("extractions must be loaded");
            let messages = self.ctx.messages().expect("messages must be loaded");
            let primary_messages = messages
                .primary_messages
                .as_ref()
                .expect("primary_messages must exist");

            let key_usages = rules::build_key_usage_map(extractions);
            let key_disable_map = rules::build_key_disable_map(extractions);

            let untranslated_issues = rules::check_untranslated(
                &self.ctx.config.primary_locale,
                primary_messages,
                &messages.all_messages,
                &key_usages,
                &key_disable_map,
            );
            all_issues.extend(untranslated_issues.into_iter().map(|i| {
                let usages = i
                    .usages
                    .iter()
                    .map(|u| {
                        crate::issue::KeyUsage::new(
                            crate::issue::SourceLocation::new(
                                &u.location.file_path,
                                u.location.line,
                            )
                            .with_col(u.location.col)
                            .with_jsx_context(u.comment_style.is_jsx()),
                        )
                    })
                    .collect::<Vec<_>>();
                let total_usages = usages.len();
                Issue::Untranslated(crate::issue::UntranslatedIssue {
                    location: crate::issue::MessageLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    ),
                    key: i.context.key,
                    value: i.context.value,
                    primary_locale: i.primary_locale,
                    identical_in: i.identical_in,
                    usages,
                    total_usages,
                })
            }));
        }

        // 9. Run type mismatch check
        if self.checks.contains(&CheckType::TypeMismatch) {
            if self.ctx.verbose {
                eprintln!("Running check: type_mismatch");
            }
            let extractions = self.ctx.all_key_usages().expect("extractions must be loaded");
            let messages = self.ctx.messages().expect("messages must be loaded");
            let primary_messages = messages
                .primary_messages
                .as_ref()
                .expect("primary_messages must exist");

            let key_usages = rules::build_key_usage_map(extractions);

            let type_mismatch_issues = rules::check_type_mismatch(
                &self.ctx.config.primary_locale,
                primary_messages,
                &messages.all_messages,
                &key_usages,
            );
            all_issues.extend(type_mismatch_issues.into_iter().map(|i| {
                let usages = i
                    .usages
                    .iter()
                    .map(|u| {
                        crate::issue::KeyUsage::new(
                            crate::issue::SourceLocation::new(
                                &u.location.file_path,
                                u.location.line,
                            )
                            .with_col(u.location.col)
                            .with_jsx_context(u.comment_style.is_jsx()),
                        )
                    })
                    .collect::<Vec<_>>();
                let total_usages = usages.len();
                Issue::TypeMismatch(crate::issue::TypeMismatchIssue {
                    location: crate::issue::MessageLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    ),
                    key: i.context.key,
                    expected_type: convert_value_type(i.expected_type),
                    primary_locale: i.primary_locale,
                    mismatched_in: i
                        .mismatched_in
                        .into_iter()
                        .map(|m| crate::issue::LocaleTypeMismatch {
                            locale: m.locale,
                            actual_type: convert_value_type(m.actual_type),
                            file_path: m.location.file_path,
                            line: m.location.line,
                        })
                        .collect(),
                    usages,
                    total_usages,
                })
            }));
        }

        // 10. Gather file counts for reporting
        let source_files_checked = self.ctx.files.len();
        let locale_files_checked = self
            .ctx
            .messages()
            .map(|m| m.all_messages.len())
            .unwrap_or(0);

        Ok(finish(
            all_issues,
            source_files_checked,
            locale_files_checked,
        ))
    }
}

/// Convert new UnresolvedKeyReason to old format.
fn convert_unresolved_reason(
    reason: &crate::types::issue::UnresolvedKeyReason,
) -> crate::types::key_usage::UnresolvedKeyReason {
    match reason {
        crate::types::issue::UnresolvedKeyReason::VariableKey => {
            crate::types::key_usage::UnresolvedKeyReason::VariableKey
        }
        crate::types::issue::UnresolvedKeyReason::TemplateWithExpr => {
            crate::types::key_usage::UnresolvedKeyReason::TemplateWithExpr
        }
        crate::types::issue::UnresolvedKeyReason::UnknownNamespace { schema_name } => {
            crate::types::key_usage::UnresolvedKeyReason::UnknownNamespace {
                schema_name: schema_name.clone(),
                raw_key: String::new(), // Not available in new format
            }
        }
    }
}

/// Convert new ValueType to old format.
fn convert_value_type(vt: crate::types::context::ValueType) -> crate::parsers::json::ValueType {
    match vt {
        crate::types::context::ValueType::String => crate::parsers::json::ValueType::String,
        crate::types::context::ValueType::StringArray => {
            crate::parsers::json::ValueType::StringArray
        }
    }
}

fn finish(
    mut issues: Vec<Issue>,
    source_files_checked: usize,
    locale_files_checked: usize,
) -> RunResult {
    issues.sort();

    let parse_error_count = issues
        .iter()
        .filter(|i| i.rule() == Rule::ParseError)
        .count();
    let error_count = issues
        .iter()
        .filter(|i| i.severity() == Severity::Error)
        .count();
    let warning_count = issues
        .iter()
        .filter(|i| i.severity() == Severity::Warning)
        .count();

    RunResult {
        error_count,
        warning_count,
        exit_on_errors: true, // check command: exit 1 on errors
        issues,
        parse_error_count,
        source_files_checked,
        locale_files_checked,
    }
}
