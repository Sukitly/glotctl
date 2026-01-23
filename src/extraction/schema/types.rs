use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct SchemaFunction {
    pub name: String,
    pub file_path: String,
    pub keys: Vec<String>,
    pub nested_calls: Vec<String>,
}

pub type SchemaRegistry = HashMap<String, SchemaFunction>;

#[derive(Debug, Clone)]
pub struct SchemaCallInfo {
    pub schema_name: String,
    pub namespace: Option<String>,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct ExpandedKey {
    pub full_key: String,
    pub raw_key: String,
    pub from_schema: String,
    pub has_namespace: bool,
}

/// Internal context used during schema function collection.
/// Moved to registry_collector module.
pub struct SchemaFunctionContext {
    pub name: String,
    pub t_param_name: String,
    pub keys: Vec<String>,
    pub nested_calls: Vec<String>,
}

/// Result of expanding schema keys
#[derive(Debug, Default)]
pub struct ExpandResult {
    pub keys: Vec<ExpandedKey>,
    /// Nested schema calls that couldn't be resolved (not in registry)
    pub unresolved_nested: Vec<String>,
}

pub fn expand_schema_keys(
    schema_name: &str,
    namespace: &Option<String>,
    registry: &SchemaRegistry,
    visited: &mut HashSet<String>,
) -> ExpandResult {
    let mut result = ExpandResult::default();

    if visited.contains(schema_name) {
        return result;
    }
    visited.insert(schema_name.to_string());

    let schema = match registry.get(schema_name) {
        Some(s) => s,
        None => return result,
    };

    for key in &schema.keys {
        let (full_key, has_namespace) = match namespace {
            Some(ns) => (format!("{}.{}", ns, key), true),
            None => (key.clone(), false),
        };
        result.keys.push(ExpandedKey {
            full_key,
            raw_key: key.clone(),
            from_schema: schema_name.to_string(),
            has_namespace,
        });
    }

    for nested_schema in &schema.nested_calls {
        if !registry.contains_key(nested_schema) {
            result.unresolved_nested.push(nested_schema.clone());
            continue;
        }
        let nested_result = expand_schema_keys(nested_schema, namespace, registry, visited);
        result.keys.extend(nested_result.keys);
        result
            .unresolved_nested
            .extend(nested_result.unresolved_nested);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::registry::RegistryCollector;
    use swc_common::FileName;
    use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};
    use swc_ecma_visit::VisitWith;

    fn parse_and_collect(code: &str) -> Vec<SchemaFunction> {
        let source_map = swc_common::SourceMap::default();
        let source_file =
            source_map.new_source_file(FileName::Real("test.ts".into()).into(), code.to_string());

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        });
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
        let module = parser.parse_module().unwrap();

        let mut collector = RegistryCollector::new("test.ts");
        module.visit_with(&mut collector);
        collector.schema_functions
    }

    #[test]
    fn test_detect_schema_function_with_tfunction_type() {
        let code = r#"
              export const createSchema = (t: TFunction) => z.object({
                  title: z.string().min(1, t("titleRequired")),
              });
          "#;

        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "createSchema");
        assert_eq!(functions[0].keys, vec!["titleRequired"]);
    }

    #[test]
    fn test_detect_schema_function_without_type() {
        let code = r#"
              export const createSchema = (t) => z.object({
                  title: z.string().min(1, t("titleRequired")),
                  desc: z.string().max(100, t("descMax")),
              });
          "#;

        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].keys, vec!["titleRequired", "descMax"]);
    }

    #[test]
    fn test_detect_tform_param() {
        let code = r#"
              export const createSchema = (tForm) => z.object({
                  name: z.string().min(1, tForm("nameRequired")),
              });
          "#;

        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].keys, vec!["nameRequired"]);
    }

    #[test]
    fn test_collect_nested_schema_calls() {
        let code = r#"
              export const createBaseSchema = (t) => z.object({
                  id: z.string().min(1, t("idRequired")),
              });

              export const createExtendedSchema = (t) =>
                  createBaseSchema(t).extend({
                      name: z.string().min(1, t("nameRequired")),
                  });
          "#;

        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 2);

        let extended = functions
            .iter()
            .find(|f| f.name == "createExtendedSchema")
            .unwrap();
        assert_eq!(extended.keys, vec!["nameRequired"]);
        assert_eq!(extended.nested_calls, vec!["createBaseSchema"]);
    }

    #[test]
    fn test_ignore_non_schema_functions() {
        let code = r#"
              export const helper = (data) => data.map(x => x);
              export const createSchema = (t) => z.object({
                  title: z.string().min(1, t("titleRequired")),
              });
          "#;

        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "createSchema");
    }

    #[test]
    fn test_expand_keys_with_namespace() {
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "createSchema".to_string(),
            SchemaFunction {
                name: "createSchema".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["titleRequired".to_string(), "descMax".to_string()],
                nested_calls: vec![],
            },
        );

        let mut visited = HashSet::new();
        let result = expand_schema_keys(
            "createSchema",
            &Some("Form".to_string()),
            &registry,
            &mut visited,
        );

        assert_eq!(result.keys.len(), 2);
        assert_eq!(result.keys[0].full_key, "Form.titleRequired");
        assert!(result.keys[0].has_namespace);
        assert_eq!(result.keys[1].full_key, "Form.descMax");
    }

    #[test]
    fn test_expand_keys_without_namespace() {
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "createSchema".to_string(),
            SchemaFunction {
                name: "createSchema".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["titleRequired".to_string()],
                nested_calls: vec![],
            },
        );

        let mut visited = HashSet::new();
        let result = expand_schema_keys("createSchema", &None, &registry, &mut visited);

        assert_eq!(result.keys.len(), 1);
        assert_eq!(result.keys[0].full_key, "titleRequired");
        assert!(!result.keys[0].has_namespace);
    }

    #[test]
    fn test_expand_nested_keys() {
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "createBase".to_string(),
            SchemaFunction {
                name: "createBase".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["baseKey".to_string()],
                nested_calls: vec![],
            },
        );
        registry.insert(
            "createExtended".to_string(),
            SchemaFunction {
                name: "createExtended".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["extendedKey".to_string()],
                nested_calls: vec!["createBase".to_string()],
            },
        );

        let mut visited = HashSet::new();
        let result = expand_schema_keys(
            "createExtended",
            &Some("Form".to_string()),
            &registry,
            &mut visited,
        );

        assert_eq!(result.keys.len(), 2);
        assert!(result.keys.iter().any(|k| k.full_key == "Form.extendedKey"));
        assert!(result.keys.iter().any(|k| k.full_key == "Form.baseKey"));
        assert!(result.unresolved_nested.is_empty());
    }

    #[test]
    fn test_circular_dependency_protection() {
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "schemaA".to_string(),
            SchemaFunction {
                name: "schemaA".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["keyA".to_string()],
                nested_calls: vec!["schemaB".to_string()],
            },
        );
        registry.insert(
            "schemaB".to_string(),
            SchemaFunction {
                name: "schemaB".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["keyB".to_string()],
                nested_calls: vec!["schemaA".to_string()], // circular!
            },
        );

        let mut visited = HashSet::new();
        let result = expand_schema_keys(
            "schemaA",
            &Some("Form".to_string()),
            &registry,
            &mut visited,
        );

        // Should not infinite loop, should have both keys
        assert_eq!(result.keys.len(), 2);
    }

    #[test]
    fn test_unresolved_nested_calls() {
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "createSchema".to_string(),
            SchemaFunction {
                name: "createSchema".to_string(),
                file_path: "test.ts".to_string(),
                keys: vec!["key1".to_string()],
                nested_calls: vec![
                    "unknownHelper".to_string(),  // not in registry
                    "anotherMissing".to_string(), // not in registry
                ],
            },
        );

        let mut visited = HashSet::new();
        let result = expand_schema_keys(
            "createSchema",
            &Some("Form".to_string()),
            &registry,
            &mut visited,
        );

        assert_eq!(result.keys.len(), 1);
        assert_eq!(result.keys[0].full_key, "Form.key1");
        assert_eq!(result.unresolved_nested.len(), 2);
        assert!(
            result
                .unresolved_nested
                .contains(&"unknownHelper".to_string())
        );
        assert!(
            result
                .unresolved_nested
                .contains(&"anotherMissing".to_string())
        );
    }

    #[test]
    fn test_detect_keys_in_long_chain() {
        let code = r#"
            export const createSchema = (t: TFunction) => z.object({
                count: z.number().int().min(1, t("countMin")).max(10, t("countMax")).optional(),
            });
        "#;
        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 1);
        assert!(
            functions[0].keys.contains(&"countMin".to_string()),
            "Expected 'countMin' in keys: {:?}",
            functions[0].keys
        );
        assert!(
            functions[0].keys.contains(&"countMax".to_string()),
            "Expected 'countMax' in keys: {:?}",
            functions[0].keys
        );
    }

    #[test]
    fn test_detect_keys_in_refine() {
        let code = r#"
            export const createSchema = (t: TFunction) => z.object({
                name: z.string(),
            }).refine(() => true, { message: t("validationError") });
        "#;
        let functions = parse_and_collect(code);
        assert_eq!(functions.len(), 1);
        assert!(
            functions[0].keys.contains(&"validationError".to_string()),
            "Expected 'validationError' in keys: {:?}",
            functions[0].keys
        );
    }
}
