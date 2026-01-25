//! Missing translation key detection rule.
//!
//! Detects translation keys used in code but not defined in the primary locale.

use crate::{
    extraction::collect::Registries,
    parsers::json::MessageMap,
    types::{context::SourceContext, issue::MissingKeyIssue, key_usage::AllKeyUsages},
};

/// Check for missing translation keys.
///
/// Finds all resolved translation keys that are not defined in the primary locale.
///
/// # Arguments
/// * `extractions` - All key usages extracted from source files
/// * `primary_messages` - Messages from the primary locale
/// * `registries` - Registries containing schema information (for schema file lookup)
///
/// # Returns
/// Vector of MissingKeyIssue for keys used but not defined
pub fn check_missing_key(
    extractions: &AllKeyUsages,
    primary_messages: &MessageMap,
    registries: &Registries,
) -> Vec<MissingKeyIssue> {
    let mut issues = Vec::new();

    for file_usages in extractions.values() {
        for resolved in &file_usages.resolved {
            let key = resolved.key.as_str();
            if !primary_messages.contains_key(key) {
                let from_schema = resolved.from_schema.as_ref().map(|s| {
                    let schema_file = registries
                        .schema
                        .get(&s.schema_name)
                        .map(|sf| sf.file_path.as_str())
                        .unwrap_or(&s.schema_file);
                    (s.schema_name.clone(), schema_file.to_string())
                });

                issues.push(MissingKeyIssue {
                    context: SourceContext::new(
                        resolved.context.location.clone(),
                        resolved.context.source_line.clone(),
                        resolved.context.comment_style,
                    ),
                    key: key.to_string(),
                    from_schema,
                });
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parsers::json::{MessageEntry, ValueType},
        types::{
            context::{CommentStyle, SourceLocation},
            key_usage::{FileKeyUsages, FullKey, ResolvedKeyUsage, SchemaSource},
        },
    };
    use std::collections::{HashMap, HashSet};

    fn create_message_map(entries: &[(&str, &str)]) -> MessageMap {
        entries
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    MessageEntry {
                        value: v.to_string(),
                        value_type: ValueType::String,
                        file_path: "en.json".to_string(),
                        line: 1,
                    },
                )
            })
            .collect()
    }

    fn create_resolved_usage(file: &str, line: usize, key: &str) -> ResolvedKeyUsage {
        ResolvedKeyUsage {
            key: FullKey::new(key),
            context: SourceContext::new(
                SourceLocation::new(file, line, 1),
                format!("t('{}')", key),
                CommentStyle::Js,
            ),
            suppressed_rules: HashSet::new(),
            from_schema: None,
        }
    }

    fn empty_registries() -> Registries {
        Registries {
            schema: HashMap::new(),
            key_object: HashMap::new(),
            key_array: HashMap::new(),
            string_array: HashMap::new(),
            translation_prop: HashMap::new(),
            translation_fn_call: HashMap::new(),
            default_exports: HashMap::new(),
        }
    }

    #[test]
    fn test_check_missing_key_none_missing() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![create_resolved_usage("test.tsx", 10, "Common.submit")],
                unresolved: vec![],
            },
        );

        let primary_messages = create_message_map(&[("Common.submit", "Submit")]);
        let registries = empty_registries();

        let issues = check_missing_key(&extractions, &primary_messages, &registries);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_missing_key_one_missing() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![
                    create_resolved_usage("test.tsx", 10, "Common.submit"),
                    create_resolved_usage("test.tsx", 20, "Common.missing"),
                ],
                unresolved: vec![],
            },
        );

        let primary_messages = create_message_map(&[("Common.submit", "Submit")]);
        let registries = empty_registries();

        let issues = check_missing_key(&extractions, &primary_messages, &registries);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "Common.missing");
        assert_eq!(issues[0].context.location.line, 20);
    }

    #[test]
    fn test_check_missing_key_multiple_files() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "a.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![create_resolved_usage("a.tsx", 1, "Key.a")],
                unresolved: vec![],
            },
        );
        extractions.insert(
            "b.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![create_resolved_usage("b.tsx", 2, "Key.b")],
                unresolved: vec![],
            },
        );

        let primary_messages = create_message_map(&[]);
        let registries = empty_registries();

        let issues = check_missing_key(&extractions, &primary_messages, &registries);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_check_missing_key_from_schema() {
        use crate::extraction::schema::SchemaFunction;

        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![ResolvedKeyUsage {
                    key: FullKey::new("Form.email"),
                    context: SourceContext::new(
                        SourceLocation::new("test.tsx", 10, 1),
                        "formSchema(t)",
                        CommentStyle::Js,
                    ),
                    suppressed_rules: HashSet::new(),
                    from_schema: Some(SchemaSource {
                        schema_name: "formSchema".to_string(),
                        schema_file: "./schemas/form.ts".to_string(),
                    }),
                }],
                unresolved: vec![],
            },
        );

        let primary_messages = create_message_map(&[]);
        let mut registries = empty_registries();
        registries.schema.insert(
            "formSchema".to_string(),
            SchemaFunction {
                name: "formSchema".to_string(),
                file_path: "./schemas/form.ts".to_string(),
                keys: vec!["email".to_string()],
                nested_calls: vec![],
            },
        );

        let issues = check_missing_key(&extractions, &primary_messages, &registries);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "Form.email");
        assert!(issues[0].from_schema.is_some());
        let (name, file) = issues[0].from_schema.as_ref().unwrap();
        assert_eq!(name, "formSchema");
        assert_eq!(file, "./schemas/form.ts");
    }

    #[test]
    fn test_check_missing_key_empty_extractions() {
        let extractions: AllKeyUsages = HashMap::new();
        let primary_messages = create_message_map(&[("Common.submit", "Submit")]);
        let registries = empty_registries();

        let issues = check_missing_key(&extractions, &primary_messages, &registries);
        assert!(issues.is_empty());
    }
}
