//! Missing translation keys detection rule.
//!
//! Detects keys used in code but not defined in locale files, including:
//! - Static keys passed to translation functions
//! - Dynamic keys with resolvable values
//! - Schema-based keys
//! - Replica lag (keys missing in non-primary locales)

use std::collections::HashSet;

use anyhow::Result;

use crate::{
    checkers::{
        extraction::DynamicKeyReason, schema::expand_schema_keys, value_source::ValueSource,
    },
    commands::{
        check::{build_key_usage_map, find_missing_keys, find_replica_lag},
        context::CheckContext,
    },
    issue::{
        DynamicKeyIssue, Issue, MissingDynamicKeyCandidatesIssue, MissingKeyIssue, SourceLocation,
        UntrackedNamespaceIssue,
    },
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

        // Process cached extractions
        for (file_path, extraction) in extractions {
            // Check missing static keys
            let missing = find_missing_keys(&extraction.used_keys, primary_messages);
            for key in missing {
                issues.push(Issue::MissingKey(MissingKeyIssue {
                    location: SourceLocation::new(&key.file_path, key.line).with_col(key.col),
                    key: key.full_key.clone(),
                    source_line: Some(key.source_line.clone()),
                    from_schema: None,
                }));
            }

            // Process dynamic key warnings
            for warning in &extraction.warnings {
                let reason = match warning.reason {
                    DynamicKeyReason::VariableKey => "dynamic key",
                    DynamicKeyReason::TemplateWithExpr => "template with expression",
                };
                issues.push(Issue::DynamicKey(DynamicKeyIssue {
                    location: SourceLocation::new(&warning.file_path, warning.line)
                        .with_col(warning.col),
                    reason: reason.to_string(),
                    source_line: Some(warning.source_line.clone()),
                    hint: warning.hint.clone(),
                }));
            }

            // Process pattern warnings
            for warning in &extraction.pattern_warnings {
                issues.push(Issue::DynamicKey(DynamicKeyIssue {
                    location: SourceLocation::new(&warning.file_path, warning.line).with_col(1),
                    reason: warning.message.clone(),
                    source_line: None,
                    hint: None,
                }));
            }

            // Process schema calls
            for call in &extraction.schema_calls {
                let mut visited = HashSet::new();
                let expand_result = expand_schema_keys(
                    &call.schema_name,
                    &call.namespace,
                    &registries.schema,
                    &mut visited,
                );
                for key in expand_result.keys {
                    if !key.has_namespace {
                        issues.push(Issue::UntrackedNamespace(UntrackedNamespaceIssue {
                            location: SourceLocation::new(file_path.as_str(), call.line)
                                .with_col(call.col),
                            raw_key: key.raw_key.clone(),
                            schema_name: key.from_schema.clone(),
                            source_line: None,
                        }));
                        continue;
                    }

                    if !primary_messages.contains_key(&key.full_key) {
                        let schema_file = registries
                            .schema
                            .get(&key.from_schema)
                            .map(|s| s.file_path.as_str())
                            .unwrap_or("unknown");

                        issues.push(Issue::MissingKey(MissingKeyIssue {
                            location: SourceLocation::new(file_path.as_str(), call.line)
                                .with_col(call.col),
                            key: key.full_key.clone(),
                            source_line: None,
                            from_schema: Some((key.from_schema.clone(), schema_file.to_string())),
                        }));
                    }
                }
            }

            // Process resolved keys
            for resolved_key in &extraction.resolved_keys {
                if matches!(resolved_key.source, ValueSource::Literal(_)) {
                    continue;
                }

                match resolved_key.source.resolve_keys() {
                    Ok(keys) => {
                        let mut missing_keys = Vec::new();
                        for key in keys {
                            let full_key = match &resolved_key.namespace {
                                Some(ns) => format!("{}.{}", ns, key),
                                None => key,
                            };
                            if !primary_messages.contains_key(&full_key) {
                                missing_keys.push(full_key);
                            }
                        }
                        if !missing_keys.is_empty() {
                            let source_desc = resolved_key.source.source_description();
                            issues.push(Issue::MissingDynamicKeyCandidates(
                                MissingDynamicKeyCandidatesIssue::new(
                                    SourceLocation::new(&resolved_key.file_path, resolved_key.line)
                                        .with_col(resolved_key.col),
                                    source_desc,
                                    missing_keys,
                                    Some(resolved_key.source_line.clone()),
                                ),
                            ));
                        }
                    }
                    Err(_) => {
                        // Cannot resolve - warnings already emitted by the checker
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
