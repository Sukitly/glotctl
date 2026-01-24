//! Translation call resolution (Phase 3).
//!
//! This module resolves raw translation calls collected in Phase 2 into
//! final ResolvedKeyUsage and UnresolvedKeyUsage results.
//!
//! It also handles schema calls expansion.

use std::collections::HashSet;

use crate::extraction::{
    collect::SuppressibleRule,
    collect::types::{FileComments, Registries},
    extract::{RawTranslationCall, TranslationCallKind, TranslationSource, ValueSource},
    schema::{SchemaCallInfo, expand_schema_keys},
};
use crate::types::context::{CommentStyle, SourceContext, SourceLocation};
use crate::types::key_usage::{
    FileKeyUsages, FullKey, ResolvedKeyUsage, SchemaSource, UnresolvedKeyReason, UnresolvedKeyUsage,
};

/// Resolve translation calls and schema calls to key usages.
///
/// This is Phase 3 of extraction, processing the raw calls and schema calls
/// collected in Phase 2. It produces:
/// - `resolved`: All keys that were successfully resolved (static, dynamic, schema)
/// - `unresolved`: Keys that could not be statically resolved (warnings)
pub fn resolve_translation_calls(
    raw_calls: &[RawTranslationCall],
    schema_calls: &[SchemaCallInfo],
    file_path: &str,
    file_comments: &FileComments,
    registries: &Registries,
    available_keys: &HashSet<String>,
) -> FileKeyUsages {
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();

    // Process raw translation calls
    for call in raw_calls {
        resolve_single_call(
            call,
            file_comments,
            available_keys,
            &mut resolved,
            &mut unresolved,
        );
    }

    // Process schema calls
    for call in schema_calls {
        resolve_schema_call(
            call,
            file_path,
            file_comments,
            registries,
            &mut resolved,
            &mut unresolved,
        );
    }

    FileKeyUsages {
        resolved,
        unresolved,
    }
}

fn resolve_single_call(
    call: &RawTranslationCall,
    file_comments: &FileComments,
    available_keys: &HashSet<String>,
    resolved: &mut Vec<ResolvedKeyUsage>,
    unresolved: &mut Vec<UnresolvedKeyUsage>,
) {
    let ctx = &call.context;
    let namespaces = call.translation_source.namespaces();

    // Collect suppressed rules for this location
    let suppressed_rules = collect_suppressed_rules(file_comments, ctx.line());

    // Try to extract static keys first
    if let Some(keys) = extract_static_keys(&call.argument) {
        for key in keys {
            for namespace in &namespaces {
                let full_key = resolve_full_key(namespace, &key);
                resolved.push(ResolvedKeyUsage {
                    key: FullKey::new(full_key),
                    context: ctx.clone(),
                    suppressed_rules: suppressed_rules.clone(),
                    from_schema: None,
                });
            }
        }
        return;
    }

    // Not a static key - try to resolve dynamically
    match call.argument.resolve_keys() {
        Ok(keys) => {
            // Dynamic but resolvable - add all resolved keys
            for key in keys {
                for namespace in &namespaces {
                    let full_key = resolve_full_key(namespace, &key);
                    resolved.push(ResolvedKeyUsage {
                        key: FullKey::new(full_key),
                        context: ctx.clone(),
                        suppressed_rules: suppressed_rules.clone(),
                        from_schema: None,
                    });
                }
            }
        }
        Err(_) => {
            // Cannot resolve - check for glot-message-keys declaration
            // Method calls don't generate warnings (preserve original behavior)
            if matches!(call.call_kind, TranslationCallKind::Method(_)) {
                return;
            }

            if let Some(decl) = file_comments.declarations.get_declaration(ctx.line()) {
                // Has declaration, expand pattern
                let expanded_keys = decl.expand_all(&namespaces, available_keys);
                for key in expanded_keys {
                    resolved.push(ResolvedKeyUsage {
                        key: FullKey::new(key),
                        context: ctx.clone(),
                        suppressed_rules: suppressed_rules.clone(),
                        from_schema: None,
                    });
                }
            } else {
                // No declaration, generate unresolved warning
                let (reason, hint, pattern) = infer_warning_details(
                    &call.argument,
                    &call.translation_source,
                    ctx.comment_style,
                );
                unresolved.push(UnresolvedKeyUsage {
                    context: ctx.clone(),
                    reason,
                    hint,
                    pattern,
                });
            }
        }
    }
}

fn resolve_schema_call(
    call: &SchemaCallInfo,
    file_path: &str,
    file_comments: &FileComments,
    registries: &Registries,
    resolved: &mut Vec<ResolvedKeyUsage>,
    unresolved: &mut Vec<UnresolvedKeyUsage>,
) {
    let mut visited = HashSet::new();
    let expand_result = expand_schema_keys(
        &call.schema_name,
        &call.namespace,
        &registries.schema,
        &mut visited,
    );

    // Build context for this schema call location
    // Note: schema calls don't have source_line in SchemaCallInfo, use empty string
    let location = SourceLocation::new(file_path, call.line, call.col);
    let context = SourceContext::new(location, "", CommentStyle::Js);

    let suppressed_rules = collect_suppressed_rules(file_comments, call.line);

    // Get schema file path for from_schema info
    let schema_file = registries
        .schema
        .get(&call.schema_name)
        .map(|s| s.file_path.clone())
        .unwrap_or_else(|| "unknown".to_string());

    for key in expand_result.keys {
        if !key.has_namespace {
            // Namespace could not be determined
            unresolved.push(UnresolvedKeyUsage {
                context: context.clone(),
                reason: UnresolvedKeyReason::UnknownNamespace {
                    schema_name: key.from_schema.clone(),
                    raw_key: key.raw_key.clone(),
                },
                hint: None,
                pattern: None,
            });
        } else {
            // Successfully resolved with namespace
            resolved.push(ResolvedKeyUsage {
                key: FullKey::new(key.full_key),
                context: context.clone(),
                suppressed_rules: suppressed_rules.clone(),
                from_schema: Some(SchemaSource {
                    schema_name: key.from_schema.clone(),
                    schema_file: schema_file.clone(),
                }),
            });
        }
    }
}

/// Collect all suppressed rules for a given line.
fn collect_suppressed_rules(
    file_comments: &FileComments,
    line: usize,
) -> HashSet<SuppressibleRule> {
    let mut suppressed = HashSet::new();
    for rule in [SuppressibleRule::Hardcoded, SuppressibleRule::Untranslated] {
        if file_comments.suppressions.is_suppressed(line, rule) {
            suppressed.insert(rule);
        }
    }
    suppressed
}

/// Extract static keys from a ValueSource.
///
/// Returns Some(keys) only for:
/// - Literal: direct string literal
/// - Conditional where BOTH branches are static (Literal or nested static Conditional)
///
/// Returns None for dynamic sources (ObjectAccess, ArrayIteration, etc.)
/// even if they can be resolved - those are handled separately.
fn extract_static_keys(source: &ValueSource) -> Option<Vec<String>> {
    match source {
        ValueSource::Literal(s) => Some(vec![s.clone()]),
        ValueSource::Conditional {
            consequent,
            alternate,
        } => {
            // Both branches must be static
            let cons_keys = extract_static_keys(consequent)?;
            let alt_keys = extract_static_keys(alternate)?;
            let mut keys = cons_keys;
            keys.extend(alt_keys);
            Some(keys)
        }
        // All other variants are "dynamic" even if resolvable
        _ => None,
    }
}

fn resolve_full_key(namespace: &Option<String>, key: &str) -> String {
    match namespace {
        Some(ns) => format!("{}.{}", ns, key),
        None => key.to_string(),
    }
}

/// Infer warning details from ValueSource.
fn infer_warning_details(
    argument: &ValueSource,
    translation_source: &TranslationSource,
    comment_style: CommentStyle,
) -> (UnresolvedKeyReason, Option<String>, Option<String>) {
    match argument {
        ValueSource::Template { prefix, suffix, .. } => {
            // Reconstruct pattern from Template
            let pattern = infer_pattern_from_template(prefix, suffix, translation_source);
            let hint = pattern.as_ref().map(|p| format_hint(p, comment_style));
            (UnresolvedKeyReason::TemplateWithExpr, hint, pattern)
        }
        ValueSource::Conditional {
            consequent,
            alternate,
        } => {
            // Check if either branch is a Template
            let is_template = matches!(consequent.as_ref(), ValueSource::Template { .. })
                || matches!(alternate.as_ref(), ValueSource::Template { .. });
            if is_template {
                (UnresolvedKeyReason::TemplateWithExpr, None, None)
            } else {
                (UnresolvedKeyReason::VariableKey, None, None)
            }
        }
        _ => (UnresolvedKeyReason::VariableKey, None, None),
    }
}

fn infer_pattern_from_template(
    prefix: &str,
    suffix: &str,
    translation_source: &TranslationSource,
) -> Option<String> {
    let pattern = format!("{}*{}", prefix, suffix);

    // Add namespace
    let full_pattern = if translation_source.is_indirect() {
        // FromProps/FromFnCall: namespace unknown
        if pattern.is_empty() || pattern == "*" {
            return None;
        }
        format!(".{}", pattern)
    } else {
        match translation_source.primary_namespace() {
            Some(ns) => format!("{}.{}", ns, pattern),
            None => pattern,
        }
    };

    // Validate pattern
    validate_pattern(&full_pattern)
}

fn validate_pattern(pattern: &str) -> Option<String> {
    let segments: Vec<&str> = pattern.split('.').collect();

    // First segment cannot be wildcard
    if segments.first().is_some_and(|s| *s == "*") && segments.len() > 1 {
        return None;
    }

    // Cannot be all wildcards
    if segments.iter().all(|s| *s == "*") {
        return None;
    }

    // Wildcard count cannot exceed 2
    if pattern.matches('*').count() > 2 {
        return None;
    }

    Some(pattern.to_string())
}

fn format_hint(pattern: &str, comment_style: CommentStyle) -> String {
    match comment_style {
        CommentStyle::Jsx => {
            format!(
                "add `{{/* glot-message-keys \"{}\" */}}` to declare expected keys",
                pattern
            )
        }
        CommentStyle::Js => {
            format!(
                "add `// glot-message-keys \"{}\"` to declare expected keys",
                pattern
            )
        }
    }
}
