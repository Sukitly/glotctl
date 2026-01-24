//! Missing translation keys detection rule.
//!
//! Detects keys used in code but not defined in locale files, including:
//! - Static keys passed to translation functions
//! - Dynamic keys with resolvable values
//! - Schema-based keys
//! - Replica lag (keys missing in non-primary locales)

use anyhow::Result;

use crate::{
    commands::{
        check::{build_key_usage_map, find_replica_lag},
        context::CheckContext,
    },
    extraction::UnresolvedKeyReason,
    issue::{Issue, MissingKeyIssue, SourceLocation, UnresolvedKeyIssue, UntrackedNamespaceIssue},
    rules::Checker,
};

pub struct MissingKeysRule;

impl Checker for MissingKeysRule {
    fn name(&self) -> &str {
        "missing_keys"
    }

    fn needs_registries(&self) -> bool {
        true
    }

    fn needs_messages(&self) -> bool {
        true
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        // Ensure extractions are loaded (this will load registries and messages first)
        ctx.ensure_extractions()?;
        let mut issues = Vec::new();

        let extractions = ctx.extractions().expect("extractions must be loaded");
        let registries = ctx.registries().expect("registries must be loaded");
        let messages = ctx.messages().expect("messages must be loaded");

        let primary_messages = match &messages.primary_messages {
            Some(m) => m,
            None => return Ok(issues),
        };

        // Process cached extractions (now FileKeyUsages)
        for file_usages in extractions.values() {
            // Check missing resolved keys
            for resolved in &file_usages.resolved {
                let key = resolved.key.as_str();
                if !primary_messages.contains_key(key) {
                    let loc = &resolved.context.location;
                    let from_schema = resolved.from_schema.as_ref().map(|s| {
                        let schema_file = registries
                            .schema
                            .get(&s.schema_name)
                            .map(|sf| sf.file_path.as_str())
                            .unwrap_or(&s.schema_file);
                        (s.schema_name.clone(), schema_file.to_string())
                    });

                    issues.push(Issue::MissingKey(MissingKeyIssue {
                        location: SourceLocation::new(&loc.file_path, loc.line).with_col(loc.col),
                        key: key.to_string(),
                        source_line: Some(resolved.context.source_line.clone()),
                        from_schema,
                    }));
                }
            }

            // Process unresolved keys
            for unresolved in &file_usages.unresolved {
                let loc = &unresolved.context.location;
                match &unresolved.reason {
                    UnresolvedKeyReason::UnknownNamespace {
                        schema_name,
                        raw_key,
                    } => {
                        issues.push(Issue::UntrackedNamespace(UntrackedNamespaceIssue {
                            location: SourceLocation::new(&loc.file_path, loc.line)
                                .with_col(loc.col),
                            raw_key: raw_key.clone(),
                            schema_name: schema_name.clone(),
                            source_line: Some(unresolved.context.source_line.clone()),
                        }));
                    }
                    _ => {
                        issues.push(Issue::UnresolvedKey(UnresolvedKeyIssue {
                            location: SourceLocation::new(&loc.file_path, loc.line)
                                .with_col(loc.col),
                            reason: unresolved.reason.clone(),
                            source_line: Some(unresolved.context.source_line.clone()),
                            hint: unresolved.hint.clone(),
                            pattern: unresolved.pattern.clone(),
                        }));
                    }
                }
            }
        }

        // Build key usage map for replica lag usages
        let key_usages = build_key_usage_map(extractions);

        // Replica lag (keys missing in non-primary locales)
        issues.extend(find_replica_lag(
            &ctx.config.primary_locale,
            &messages.all_messages,
            &key_usages,
        ));

        Ok(issues)
    }
}
