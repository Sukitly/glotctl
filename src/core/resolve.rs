//! Phase 3: Resolution - Resolving raw calls to final key usages.
//!
//! This module handles the third and final phase of the analysis pipeline:
//! resolving raw translation calls collected in Phase 2 into final ResolvedKeyUsage
//! and UnresolvedKeyUsage results.
//!
//! Resolution includes:
//! - Static key extraction from literals and conditionals
//! - Dynamic key resolution from object access and array iteration
//! - Schema function expansion
//! - glot-message-keys pattern expansion
//! - Unresolved key warning generation

use std::collections::HashSet;

use std::collections::HashMap;

use crate::core::{CommentStyle, SourceContext, SourceLocation};
use crate::core::{
    FileKeyUsages, FullKey, ResolvedKeyUsage, SchemaSource, UnresolvedKeyUsage,
    UsageUnresolvedKeyReason,
};
use crate::core::{
    collect::SuppressibleRule,
    collect::types::FileComments,
    extract::{RawTranslationCall, TranslationCallKind, TranslationSource, ValueSource},
    schema::{ExpandResult, SchemaCallInfo, SchemaRegistry},
};

/// Resolve translation calls and schema calls to key usages.
///
/// This is Phase 3 of the analysis pipeline, processing the raw calls and schema calls
/// collected in Phase 2. It produces:
/// - `resolved`: All keys that were successfully resolved (static, dynamic, schema, glot-message-keys)
/// - `unresolved`: Keys that could not be statically resolved (dynamic key warnings)
pub fn resolve_translation_calls(
    raw_calls: &[RawTranslationCall],
    schema_calls: &[SchemaCallInfo],
    file_path: &str,
    file_comments: &FileComments,
    schema_cache: &HashMap<String, ExpandResult>,
    schema_registry: &SchemaRegistry,
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
            schema_cache,
            schema_registry,
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
    schema_cache: &HashMap<String, ExpandResult>,
    schema_registry: &SchemaRegistry,
    resolved: &mut Vec<ResolvedKeyUsage>,
    unresolved: &mut Vec<UnresolvedKeyUsage>,
) {
    // Look up pre-computed expansion (O(1) HashMap lookup)
    let cached_result = match schema_cache.get(&call.schema_name) {
        Some(result) => result,
        None => {
            // Schema not in registry (shouldn't happen if Phase 1 worked correctly)
            return;
        }
    };

    let location = SourceLocation::new(file_path, call.line, call.col);
    let context = SourceContext::new(location, "", CommentStyle::Js);
    let suppressed_rules = collect_suppressed_rules(file_comments, call.line);

    // Get schema file path for from_schema info
    let schema_file = schema_registry
        .get(&call.schema_name)
        .map(|s| s.file_path.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Apply namespace to cached raw_keys
    for cached_key in &cached_result.keys {
        let full_key = match &call.namespace {
            Some(ns) => {
                // Apply namespace: "Form" + "titleRequired" → "Form.titleRequired"
                format!("{}.{}", ns, cached_key.raw_key)
            }
            None => {
                // No namespace from call site, use raw_key as-is
                cached_key.raw_key.clone()
            }
        };

        let has_namespace = call.namespace.is_some();

        if !has_namespace {
            // Namespace could not be determined at call site
            unresolved.push(UnresolvedKeyUsage {
                context: context.clone(),
                reason: UsageUnresolvedKeyReason::UnknownNamespace {
                    schema_name: call.schema_name.clone(),
                    raw_key: cached_key.raw_key.clone(),
                },
                hint: None,
                pattern: None,
            });
        } else {
            resolved.push(ResolvedKeyUsage {
                key: FullKey::new(full_key),
                context: context.clone(),
                suppressed_rules: suppressed_rules.clone(),
                from_schema: Some(SchemaSource {
                    schema_name: call.schema_name.clone(),
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
) -> (UsageUnresolvedKeyReason, Option<String>, Option<String>) {
    match argument {
        ValueSource::Template { prefix, suffix, .. } => {
            // Reconstruct pattern from Template
            let pattern = infer_pattern_from_template(prefix, suffix, translation_source);
            let hint = pattern.as_ref().map(|p| format_hint(p, comment_style));
            (UsageUnresolvedKeyReason::TemplateWithExpr, hint, pattern)
        }
        ValueSource::Conditional {
            consequent,
            alternate,
        } => {
            // Check if either branch is a Template
            let is_template = matches!(consequent.as_ref(), ValueSource::Template { .. })
                || matches!(alternate.as_ref(), ValueSource::Template { .. });
            if is_template {
                (UsageUnresolvedKeyReason::TemplateWithExpr, None, None)
            } else {
                (UsageUnresolvedKeyReason::VariableKey, None, None)
            }
        }
        _ => (UsageUnresolvedKeyReason::VariableKey, None, None),
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
