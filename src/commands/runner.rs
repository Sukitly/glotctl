use anyhow::{Result, bail};
use clap::ValueEnum;

use crate::{
    RunResult,
    args::CheckArgs,
    commands::context::CheckContext,
    issue::{Issue, IssueReport, Rule, Severity},
    rules::{
        Checker, hardcoded::HardcodedRule, missing::MissingKeysRule, orphan::OrphanKeysRule,
        type_mismatch::TypeMismatchRule, untranslated::UntranslatedRule,
    },
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
/// 2. Managing a list of Checkers (rules) to execute
/// 3. Lazy-loading data via CheckContext based on Checker requirements
/// 4. Aggregating issues from all Checkers
pub struct CheckRunner {
    checkers: Vec<Box<dyn Checker>>,
    ctx: CheckContext,
}

impl CheckRunner {
    pub fn new(args: CheckArgs) -> Result<Self> {
        let ctx = CheckContext::new(&args.common)?;
        Ok(Self {
            checkers: Vec::new(),
            ctx,
        })
    }

    pub fn add(mut self, check: CheckType) -> Self {
        let name = match check {
            CheckType::Hardcoded => "hardcoded",
            CheckType::Missing => "missing_keys",
            CheckType::Orphan => "orphan_keys",
            CheckType::Untranslated => "untranslated",
            CheckType::TypeMismatch => "type_mismatch",
        };

        // Deduplicate: skip if already added
        if self.checkers.iter().any(|c| c.name() == name) {
            return self;
        }

        match check {
            CheckType::Hardcoded => self.checkers.push(Box::new(HardcodedRule)),
            CheckType::Missing => self.checkers.push(Box::new(MissingKeysRule)),
            CheckType::Orphan => self.checkers.push(Box::new(OrphanKeysRule)),
            CheckType::Untranslated => self.checkers.push(Box::new(UntranslatedRule)),
            CheckType::TypeMismatch => self.checkers.push(Box::new(TypeMismatchRule)),
        }
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
        // 1. Determine requirements
        let needs_registries = self.checkers.iter().any(|c| c.needs_registries());
        let needs_messages = self.checkers.iter().any(|c| c.needs_messages());

        let mut all_issues = Vec::new();

        // 2. Pre-load shared data
        if needs_registries {
            let mut parse_errors = self.ctx.ensure_registries()?;
            all_issues.append(&mut parse_errors);
        }

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

        // 3. Execute checkers
        for checker in &self.checkers {
            // Use verbose logging to trace execution
            if self.ctx.verbose {
                eprintln!("Running checker: {}", checker.name());
            }
            let mut issues = checker.check(&self.ctx)?;
            all_issues.append(&mut issues);
        }

        // 4. Gather file counts for reporting
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
