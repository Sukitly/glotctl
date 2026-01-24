//! Translation call resolution (Phase 3).
//!
//! This module resolves raw translation calls collected in Phase 2 into
//! final UsedKey and DynamicKeyWarning results.

use std::collections::HashSet;

use crate::extraction::extract::ResolvedKey;
use crate::extraction::{
    collect::SuppressibleRule,
    collect::types::FileComments,
    extract::{RawTranslationCall, TranslationCallKind, TranslationSource, ValueSource},
    results::{DynamicKeyReason, DynamicKeyWarning, UsedKey},
};
use crate::types::context::CommentStyle;

/// Result of resolving translation calls for a single file.
pub struct ResolveResult {
    pub used_keys: Vec<UsedKey>,
    pub warnings: Vec<DynamicKeyWarning>,
    pub resolved_keys: Vec<ResolvedKey>,
}

/// Resolve translation calls to used keys and warnings.
///
/// This is Phase 3 of extraction, processing the raw calls collected in Phase 2.
pub fn resolve_translation_calls(
    raw_calls: &[RawTranslationCall],
    file_comments: &FileComments,
    available_keys: &HashSet<String>,
) -> ResolveResult {
    let mut used_keys = Vec::new();
    let mut warnings = Vec::new();
    let mut resolved_keys = Vec::new();

    for call in raw_calls {
        // Build ResolvedKey for compatibility
        let namespace = call.translation_source.primary_namespace();
        resolved_keys.push(ResolvedKey {
            file_path: call.context.file_path().to_string(),
            line: call.context.line(),
            col: call.context.col(),
            source_line: call.context.source_line.clone(),
            namespace: namespace.clone(),
            source: call.argument.clone(),
        });

        resolve_single_call(
            call,
            file_comments,
            available_keys,
            &mut used_keys,
            &mut warnings,
        );
    }

    ResolveResult {
        used_keys,
        warnings,
        resolved_keys,
    }
}

fn resolve_single_call(
    call: &RawTranslationCall,
    file_comments: &FileComments,
    available_keys: &HashSet<String>,
    used_keys: &mut Vec<UsedKey>,
    warnings: &mut Vec<DynamicKeyWarning>,
) {
    let ctx = &call.context;
    let namespaces = call.translation_source.namespaces();
    let is_suppressed = file_comments
        .suppressions
        .is_suppressed(ctx.line(), SuppressibleRule::Untranslated);

    // Only add STATIC keys to used_keys.
    // Dynamic keys that can be resolved (ObjectAccess, ArrayIteration, etc.)
    // are handled via resolved_keys in rules/missing.rs.
    if let Some(keys) = extract_static_keys(&call.argument) {
        for key in keys {
            for namespace in &namespaces {
                let full_key = resolve_full_key(namespace, &key);
                add_used_key(used_keys, ctx, full_key, is_suppressed);
            }
        }
        return;
    }

    // Not a static key - check if resolvable (for resolved_keys processing in rules/missing.rs)
    let is_resolvable = call.argument.resolve_keys().is_ok();

    if is_resolvable {
        // Resolvable but not static - handled by resolved_keys in rules/missing.rs
        // Don't add to used_keys, don't generate warning
        return;
    }

    // Cannot resolve - check for glot-message-keys declaration or generate warning
    // Method calls don't generate warnings (preserve original behavior)
    if matches!(call.call_kind, TranslationCallKind::Method(_)) {
        return;
    }

    if let Some(decl) = file_comments.declarations.get_declaration(ctx.line()) {
        // Has declaration, expand pattern
        let expanded_keys = decl.expand_all(&namespaces, available_keys);
        for key in expanded_keys {
            add_used_key(used_keys, ctx, key, is_suppressed);
        }
    } else {
        // No declaration, generate warning
        let (reason, hint, pattern) =
            infer_warning_details(&call.argument, &call.translation_source, ctx.comment_style);
        add_warning(warnings, ctx, reason, hint, pattern);
    }
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

fn add_used_key(
    used_keys: &mut Vec<UsedKey>,
    ctx: &crate::types::context::SourceContext,
    full_key: String,
    is_suppressed: bool,
) {
    used_keys.push(UsedKey {
        full_key,
        file_path: ctx.file_path().to_string(),
        line: ctx.line(),
        col: ctx.col(),
        source_line: ctx.source_line.clone(),
        in_jsx_context: ctx.comment_style.is_jsx(),
        untranslated_disabled: is_suppressed,
    });
}

fn add_warning(
    warnings: &mut Vec<DynamicKeyWarning>,
    ctx: &crate::types::context::SourceContext,
    reason: DynamicKeyReason,
    hint: Option<String>,
    pattern: Option<String>,
) {
    warnings.push(DynamicKeyWarning {
        file_path: ctx.file_path().to_string(),
        line: ctx.line(),
        col: ctx.col(),
        reason,
        source_line: ctx.source_line.clone(),
        hint,
        pattern,
        in_jsx_context: ctx.comment_style.is_jsx(),
    });
}

/// Infer warning details from ValueSource.
fn infer_warning_details(
    argument: &ValueSource,
    translation_source: &TranslationSource,
    comment_style: CommentStyle,
) -> (DynamicKeyReason, Option<String>, Option<String>) {
    match argument {
        ValueSource::Template { prefix, suffix, .. } => {
            // Reconstruct pattern from Template
            let pattern = infer_pattern_from_template(prefix, suffix, translation_source);
            let hint = pattern.as_ref().map(|p| format_hint(p, comment_style));
            (DynamicKeyReason::TemplateWithExpr, hint, pattern)
        }
        ValueSource::Conditional {
            consequent,
            alternate,
        } => {
            // Check if either branch is a Template
            let is_template = matches!(consequent.as_ref(), ValueSource::Template { .. })
                || matches!(alternate.as_ref(), ValueSource::Template { .. });
            if is_template {
                (DynamicKeyReason::TemplateWithExpr, None, None)
            } else {
                (DynamicKeyReason::VariableKey, None, None)
            }
        }
        _ => (DynamicKeyReason::VariableKey, None, None),
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
