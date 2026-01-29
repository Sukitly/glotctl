//! Unified value source representation for dynamic key analysis (Phase 2: Extraction).
//!
//! This module provides a unified way to represent and resolve dynamic translation keys.
//! Instead of having separate handling for each pattern (object access, array iteration, etc.),
//! all dynamic expressions are analyzed into a common `ValueSource` enum that can be
//! recursively resolved to candidate string values.
//!
//! # Phase Context
//!
//! - **Created in**: Phase 2 (Extraction) by `ValueAnalyzer` while analyzing translation call arguments
//! - **Consumed in**: Phase 3 (Resolution) to determine if dynamic keys exist in locale files
//!
//! # Examples
//!
//! ```ignore
//! // Literal key
//! t("home.title") → ValueSource::Literal("home.title")
//!
//! // Template with known variable
//! t(`prefix.${key}`) where key ∈ ["a", "b"]
//! → ValueSource::Template { prefix: "prefix.", inner: StringArrayElement(...), ... }
//! → Resolves to ["prefix.a", "prefix.b"]
//!
//! // Conditional with static branches
//! t(flag ? "key.plural" : "key.singular")
//! → ValueSource::Conditional { consequent: Literal("key.plural"), alternate: Literal("key.singular") }
//! → Resolves to ["key.plural", "key.singular"]
//! ```

/// Represents the possible values an expression can resolve to.
///
/// This is the core abstraction for the unified value tracing system.
/// All dynamic key expressions are analyzed into this enum, which can then
/// be recursively resolved to produce candidate translation keys.
///
/// The enum forms a tree structure where composite variants (`Template`, `Conditional`)
/// can contain nested `ValueSource` values, allowing complex expressions to be represented.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueSource {
    /// A known static string literal: `"keyA"`
    ///
    /// This is the simplest case - a hardcoded string that's known at build time.
    Literal(String),

    /// A template literal: `` `prefix.${inner}.suffix` ``
    ///
    /// The inner `ValueSource` represents what `${expr}` can resolve to.
    /// The final keys are computed as: `prefix + inner_value + suffix` for each inner value.
    ///
    /// Currently only supports single-expression templates. Multi-expression templates
    /// like `` `${a}.${b}` `` are marked as `Unresolvable::ComplexTemplate`.
    Template {
        /// Text before the interpolation expression.
        prefix: String,
        /// Text after the interpolation expression.
        suffix: String,
        /// The interpolated expression (can be any `ValueSource`, including nested templates).
        inner: Box<ValueSource>,
    },

    /// A conditional expression: `cond ? a : b`
    ///
    /// Both branches are analyzed and their candidate values are combined.
    /// If either branch is unresolvable, the entire conditional is unresolvable.
    Conditional {
        /// The "then" branch (resolved recursively).
        consequent: Box<ValueSource>,
        /// The "else" branch (resolved recursively).
        alternate: Box<ValueSource>,
    },

    /// Object property access: `obj[key]` resolves to all values of the object.
    ///
    /// For example, `toolKeys[toolName]` where `toolKeys = { create: "keyA", edit: "keyB" }`
    /// resolves to `["keyA", "keyB"]`. The actual property key doesn't matter - we return
    /// all possible values.
    ///
    /// The object must be registered in Phase 1 (Collection) for this to work.
    ObjectAccess {
        /// Name of the object variable (e.g., "toolKeys").
        object_name: String,
        /// All string values from the object (collected in Phase 1).
        candidate_values: Vec<String>,
    },

    /// Array iteration accessing a property: `arr.map(item => item.prop)`
    ///
    /// For example, `capabilities.map(cap => cap.titleKey)` where
    /// `capabilities = [{ titleKey: "a" }, { titleKey: "b" }]`
    /// resolves to `["a", "b"]`.
    ///
    /// The array must be registered in Phase 1 (Collection) for this to work.
    ArrayIteration {
        /// Name of the array variable (e.g., "capabilities").
        array_name: String,
        /// Name of the property being accessed (e.g., "titleKey").
        property_name: String,
        /// All values of the property from array elements (collected in Phase 1).
        candidate_values: Vec<String>,
    },

    /// String array element: `KEYS.map(k => k)` or `KEYS[0]`
    ///
    /// For example, `FEATURE_KEYS.map(k => k)` where
    /// `FEATURE_KEYS = ["save", "load"]`
    /// resolves to `["save", "load"]`.
    ///
    /// The array must be registered in Phase 1 (Collection) for this to work.
    StringArrayElement {
        /// Name of the string array variable (e.g., "FEATURE_KEYS").
        array_name: String,
        /// All string elements from the array (collected in Phase 1).
        candidate_values: Vec<String>,
    },

    /// Cannot resolve - the expression is truly dynamic or unsupported.
    ///
    /// This becomes an `UnresolvedKeyUsage` in Phase 3 with the given reason.
    Unresolvable {
        /// Why the value source cannot be resolved.
        reason: UnresolvableReason,
    },
}

/// Reasons why a value source cannot be resolved.
///
/// Each variant maps to a specific `UnresolvedKeyReason` in Phase 3,
/// which determines whether it's reported as a warning or error.
///
/// See also: `crate::issues::UnresolvedKeyReason` (user-facing enum)
/// and `crate::core::key_usage::UnresolvedKeyReason` (internal enum).
#[derive(Debug, Clone, PartialEq)]
pub enum UnresolvableReason {
    /// Variable comes from an unknown source (e.g., function parameters, external imports).
    ///
    /// Example: `t(someParam)` where `someParam` is a function parameter.
    ///
    /// **Phase 3 outcome**: Warning (dynamic-key)
    UnknownVariable(String),

    /// Referenced object is not in the registry (wasn't found during Phase 1).
    ///
    /// Example: `t(obj[key])` where `obj` was imported or defined outside the file.
    ///
    /// **Phase 3 outcome**: Warning (dynamic-key)
    UnknownObject(String),

    /// Referenced array is not in the registry (wasn't found during Phase 1).
    ///
    /// Example: `t(arr[0])` where `arr` was imported or defined outside the file.
    ///
    /// **Phase 3 outcome**: Warning (dynamic-key)
    UnknownArray(String),

    /// Template has multiple expressions (e.g., `` `${a}.${b}` ``).
    ///
    /// Currently we only support single-expression templates. Multi-expression
    /// templates would require tracking relationships between variables.
    ///
    /// **Phase 3 outcome**: Warning (dynamic-key)
    ComplexTemplate {
        /// Number of interpolation expressions in the template.
        expr_count: usize,
    },

    /// Expression type is not supported for analysis.
    ///
    /// Example: `t(Math.random() > 0.5 ? "a" : "b")` (binary expression in template).
    ///
    /// **Phase 3 outcome**: Warning (dynamic-key)
    UnsupportedExpression {
        /// Expression type name for debugging (e.g., "BinaryExpression", "CallExpression").
        expr_type: String,
    },
}

/// Legacy structure from before the Phase 2/3 split.
///
/// This type is no longer used - it was replaced by `RawTranslationCall` (Phase 2 output)
/// and `ResolvedKeyUsage`/`UnresolvedKeyUsage` (Phase 3 output).
///
/// Kept for compatibility during refactoring, marked as dead_code.
#[derive(Debug, Clone)]
#[allow(dead_code)]
#[deprecated(note = "Use RawTranslationCall + ResolvedKeyUsage/UnresolvedKeyUsage instead")]
pub struct ResolvedKey {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub source_line: String,
    pub namespace: Option<String>,
    pub source: ValueSource,
}

impl ValueSource {
    /// Flatten the `ValueSource` into all possible string keys.
    ///
    /// This recursively resolves the value source tree and produces all
    /// candidate translation keys.
    ///
    /// # Examples
    ///
    /// - `Literal("key")` → `Ok(["key"])`
    /// - `Template { prefix: "a.", suffix: ".b", inner: Literal("x") }` → `Ok(["a.x.b"])`
    /// - `Conditional { Literal("x"), Literal("y") }` → `Ok(["x", "y"])`
    /// - `Unresolvable { reason }` → `Err(reason)`
    pub fn resolve_keys(&self) -> Result<Vec<String>, UnresolvableReason> {
        match self {
            ValueSource::Literal(s) => Ok(vec![s.clone()]),

            ValueSource::Template {
                prefix,
                suffix,
                inner,
            } => {
                let inner_keys = inner.resolve_keys()?;
                Ok(inner_keys
                    .into_iter()
                    .map(|k| format!("{}{}{}", prefix, k, suffix))
                    .collect())
            }

            ValueSource::Conditional {
                consequent,
                alternate,
            } => {
                // Try to resolve both branches
                let cons_result = consequent.resolve_keys();
                let alt_result = alternate.resolve_keys();

                match (cons_result, alt_result) {
                    // Both resolved successfully - merge candidates
                    (Ok(mut cons_keys), Ok(alt_keys)) => {
                        cons_keys.extend(alt_keys);
                        Ok(cons_keys)
                    }
                    // Both branches must resolve for validation.
                    // Rationale: If we can't determine all possible keys, we can't
                    // guarantee they all exist in the message files. Partial results
                    // would give false confidence that translations are complete.
                    (Err(reason), _) | (_, Err(reason)) => Err(reason),
                }
            }

            ValueSource::ObjectAccess {
                candidate_values, ..
            } => Ok(candidate_values.clone()),

            ValueSource::ArrayIteration {
                candidate_values, ..
            } => Ok(candidate_values.clone()),

            ValueSource::StringArrayElement {
                candidate_values, ..
            } => Ok(candidate_values.clone()),

            ValueSource::Unresolvable { reason } => Err(reason.clone()),
        }
    }

    /// Get a human-readable description of the value source for error messages.
    #[allow(dead_code)]
    pub fn source_description(&self) -> String {
        match self {
            ValueSource::Literal(s) => format!("literal \"{}\"", s),
            ValueSource::Template { .. } => "template".to_string(),
            ValueSource::Conditional { .. } => "conditional".to_string(),
            ValueSource::ObjectAccess { object_name, .. } => {
                format!("object \"{}\"", object_name)
            }
            ValueSource::ArrayIteration {
                array_name,
                property_name,
                ..
            } => {
                format!("array \"{}.{}\"", array_name, property_name)
            }
            ValueSource::StringArrayElement { array_name, .. } => {
                format!("array \"{}\"", array_name)
            }
            ValueSource::Unresolvable { reason } => match reason {
                UnresolvableReason::UnknownVariable(v) => format!("unknown variable \"{}\"", v),
                UnresolvableReason::UnknownObject(o) => format!("unknown object \"{}\"", o),
                UnresolvableReason::UnknownArray(a) => format!("unknown array \"{}\"", a),
                UnresolvableReason::ComplexTemplate { expr_count } => {
                    format!("complex template with {} expressions", expr_count)
                }
                UnresolvableReason::UnsupportedExpression { expr_type } => {
                    format!("unsupported expression: {}", expr_type)
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::extract::value_source::*;

    #[test]
    fn test_literal_resolves_to_itself() {
        let source = ValueSource::Literal("key".to_string());
        assert_eq!(source.resolve_keys(), Ok(vec!["key".to_string()]));
    }

    #[test]
    fn test_template_combines_prefix_suffix() {
        let source = ValueSource::Template {
            prefix: "prefix.".to_string(),
            suffix: ".suffix".to_string(),
            inner: Box::new(ValueSource::Literal("middle".to_string())),
        };
        assert_eq!(
            source.resolve_keys(),
            Ok(vec!["prefix.middle.suffix".to_string()])
        );
    }

    #[test]
    fn test_template_with_multiple_inner_values() {
        let source = ValueSource::Template {
            prefix: "ns.".to_string(),
            suffix: "".to_string(),
            inner: Box::new(ValueSource::StringArrayElement {
                array_name: "KEYS".to_string(),
                candidate_values: vec!["a".to_string(), "b".to_string()],
            }),
        };
        assert_eq!(
            source.resolve_keys(),
            Ok(vec!["ns.a".to_string(), "ns.b".to_string()])
        );
    }

    #[test]
    fn test_conditional_merges_branches() {
        let source = ValueSource::Conditional {
            consequent: Box::new(ValueSource::Literal("keyA".to_string())),
            alternate: Box::new(ValueSource::Literal("keyB".to_string())),
        };
        let result = source.resolve_keys().unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"keyA".to_string()));
        assert!(result.contains(&"keyB".to_string()));
    }

    #[test]
    fn test_conditional_fails_if_one_branch_unresolvable() {
        let source = ValueSource::Conditional {
            consequent: Box::new(ValueSource::Literal("keyA".to_string())),
            alternate: Box::new(ValueSource::Unresolvable {
                reason: UnresolvableReason::UnknownVariable("x".to_string()),
            }),
        };
        assert!(source.resolve_keys().is_err());
    }

    #[test]
    fn test_object_access_returns_candidate_values() {
        let source = ValueSource::ObjectAccess {
            object_name: "toolKeys".to_string(),
            candidate_values: vec!["keyA".to_string(), "keyB".to_string()],
        };
        assert_eq!(
            source.resolve_keys(),
            Ok(vec!["keyA".to_string(), "keyB".to_string()])
        );
    }

    #[test]
    fn test_unresolvable_returns_error() {
        let source = ValueSource::Unresolvable {
            reason: UnresolvableReason::UnknownVariable("x".to_string()),
        };
        assert_eq!(
            source.resolve_keys(),
            Err(UnresolvableReason::UnknownVariable("x".to_string()))
        );
    }

    #[test]
    fn test_nested_template_with_conditional() {
        // t(flag ? `${k}.plural` : `${k}.singular`)
        // where k comes from KEYS = ["a", "b"]
        let string_array = ValueSource::StringArrayElement {
            array_name: "KEYS".to_string(),
            candidate_values: vec!["a".to_string(), "b".to_string()],
        };

        let source = ValueSource::Conditional {
            consequent: Box::new(ValueSource::Template {
                prefix: "".to_string(),
                suffix: ".plural".to_string(),
                inner: Box::new(string_array.clone()),
            }),
            alternate: Box::new(ValueSource::Template {
                prefix: "".to_string(),
                suffix: ".singular".to_string(),
                inner: Box::new(string_array),
            }),
        };

        let result = source.resolve_keys().unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.contains(&"a.plural".to_string()));
        assert!(result.contains(&"b.plural".to_string()));
        assert!(result.contains(&"a.singular".to_string()));
        assert!(result.contains(&"b.singular".to_string()));
    }

    #[test]
    fn test_source_description() {
        assert_eq!(
            ValueSource::Literal("key".to_string()).source_description(),
            "literal \"key\""
        );

        assert_eq!(
            ValueSource::ObjectAccess {
                object_name: "obj".to_string(),
                candidate_values: vec![],
            }
            .source_description(),
            "object \"obj\""
        );

        assert_eq!(
            ValueSource::Unresolvable {
                reason: UnresolvableReason::ComplexTemplate { expr_count: 3 },
            }
            .source_description(),
            "complex template with 3 expressions"
        );

        assert_eq!(
            ValueSource::Unresolvable {
                reason: UnresolvableReason::UnsupportedExpression {
                    expr_type: "Arrow".to_string(),
                },
            }
            .source_description(),
            "unsupported expression: Arrow"
        );
    }
}
