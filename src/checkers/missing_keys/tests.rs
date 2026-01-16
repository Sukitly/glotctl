//! Tests for missing key detection.

use std::collections::{HashMap, HashSet};
use swc_common::FileName;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};
use swc_ecma_visit::Visit;

use super::*;
use crate::checkers::key_objects::{
    FileImports, KeyArray, KeyArrayRegistry, KeyObject, KeyObjectRegistry, StringArrayRegistry,
    make_registry_key,
};
use crate::checkers::schema::SchemaRegistry;
use crate::commands::context::Registries;

fn create_empty_registries() -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object: KeyObjectRegistry::new(),
        key_array: KeyArrayRegistry::new(),
        string_array: StringArrayRegistry::new(),
    }
}

fn create_registries_with_key_objects(key_object: KeyObjectRegistry) -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object,
        key_array: KeyArrayRegistry::new(),
        string_array: StringArrayRegistry::new(),
    }
}

fn parse_and_check(code: &str) -> MissingKeyChecker<'static> {
    let source_map = Box::leak(Box::new(swc_common::SourceMap::default()));
    let source_file =
        source_map.new_source_file(FileName::Real("test.tsx".into()).into(), code.to_string());

    let syntax = Syntax::Typescript(TsSyntax {
        tsx: true,
        ..Default::default()
    });
    let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
    let module = parser.parse_module().unwrap();

    let file_path = Box::leak(Box::new("test.tsx".to_string()));
    let registries = Box::leak(Box::new(create_empty_registries()));
    let file_imports = Box::leak(Box::new(FileImports::new()));
    let available_keys = HashSet::new();
    let mut checker = MissingKeyChecker::new(
        file_path,
        source_map,
        registries,
        file_imports,
        code,
        &available_keys,
    );
    checker.visit_module(&module);
    checker
}

#[test]
fn test_use_translations_with_namespace() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button() {
              return <button>{t("submit")}</button>;
          }
      "#;
    let checker = parse_and_check(code);

    assert_eq!(checker.used_keys.len(), 1);
    assert_eq!(checker.used_keys[0].full_key, "Common.submit");
}

#[test]
fn test_use_translations_without_namespace() {
    let code = r#"
          const t = useTranslations();
          export function Button() {
              return <button>{t("Common.submit")}</button>;
          }
      "#;
    let checker = parse_and_check(code);

    assert_eq!(checker.used_keys.len(), 1);
    assert_eq!(checker.used_keys[0].full_key, "Common.submit");
}

#[test]
fn test_await_get_translations() {
    let code = r#"
          const t = await getTranslations("Auth");
          export function Page() {
              return <div>{t("login")}</div>;
          }
      "#;
    let checker = parse_and_check(code);

    assert_eq!(checker.used_keys.len(), 1);
    assert_eq!(checker.used_keys[0].full_key, "Auth.login");
}

#[test]
fn test_renamed_function() {
    let code = r#"
          const translate = useTranslations("Common");
          export function Button() {
              return <button>{translate("submit")}</button>;
          }
      "#;
    let checker = parse_and_check(code);

    assert_eq!(checker.used_keys.len(), 1);
    assert_eq!(checker.used_keys[0].full_key, "Common.submit");
}

#[test]
fn test_dynamic_key_warning() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button({ keyName }) {
              return <button>{t(keyName)}</button>;
          }
      "#;
    let checker = parse_and_check(code);

    assert!(checker.used_keys.is_empty());
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::VariableKey
    ));
}

#[test]
fn test_template_with_expr_warning() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button({ prefix }) {
              return <button>{t(`${prefix}.submit`)}</button>;
          }
      "#;
    let checker = parse_and_check(code);

    assert!(checker.used_keys.is_empty());
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::TemplateWithExpr
    ));
}

#[test]
fn test_template_with_expr_hint_jsx_context() {
    let code = r#"
        import {useTranslations} from 'next-intl';
        export function Component() {
            const t = useTranslations('Common');
            return <div>{t(`items.${id}`)}</div>;
        }
    "#;
    let checker = parse_and_check(code);
    assert_eq!(checker.warnings.len(), 1);
    let hint = checker.warnings[0].hint.as_ref().unwrap();
    assert!(hint.contains("{/* glot-message-keys"));
    assert!(hint.contains("*/}"));
}

#[test]
fn test_template_with_expr_hint_non_jsx_context() {
    let code = r#"
        import {useTranslations} from 'next-intl';
        export function Component() {
            const t = useTranslations('Common');
            const message = t(`items.${id}`);
            return <div>{message}</div>;
        }
    "#;
    let checker = parse_and_check(code);
    assert_eq!(checker.warnings.len(), 1);
    let hint = checker.warnings[0].hint.as_ref().unwrap();
    assert!(hint.contains("// glot-message-keys"));
    assert!(!hint.contains("{/*"));
}

#[test]
fn test_template_with_expr_hint_jsx_prop() {
    let code = r#"
        import {useTranslations} from 'next-intl';
        export function Component() {
            const t = useTranslations('Common');
            return <input placeholder={t(`hint.${type}`)} />;
        }
    "#;
    let checker = parse_and_check(code);
    assert_eq!(checker.warnings.len(), 1);
    let hint = checker.warnings[0].hint.as_ref().unwrap();
    // Props should use // style, not {/* */}
    assert!(hint.contains("// glot-message-keys"));
    assert!(!hint.contains("{/*"));
}

#[test]
fn test_multiple_keys() {
    let code = r#"
          const t = useTranslations("Common");
          export function Form() {
              return (
                  <form>
                      <button>{t("submit")}</button>
                      <button>{t("cancel")}</button>
                  </form>
              );
          }
      "#;
    let checker = parse_and_check(code);

    assert_eq!(checker.used_keys.len(), 2);
    assert_eq!(checker.used_keys[0].full_key, "Common.submit");
    assert_eq!(checker.used_keys[1].full_key, "Common.cancel");
}

#[test]
fn test_key_with_dot_prepends_namespace() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button() {
              return <button>{t("Button.submit")}</button>;
          }
      "#;
    let checker = parse_and_check(code);

    assert_eq!(checker.used_keys.len(), 1);
    assert_eq!(checker.used_keys[0].full_key, "Common.Button.submit");
}

#[test]
fn test_ternary_with_string_literals() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ approved }) {
              return <span>{t(approved ? "approved" : "rejected")}</span>;
          }
      "#;
    let checker = parse_and_check(code);

    // Both keys should be extracted (use HashSet for order-independent comparison)
    let keys: HashSet<_> = checker
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains("Common.approved"));
    assert!(keys.contains("Common.rejected"));
    // No warnings
    assert!(checker.warnings.is_empty());
}

#[test]
fn test_ternary_with_mixed_branches() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ key, flag }) {
              return <span>{t(flag ? key : "fallback")}</span>;
          }
      "#;
    let checker = parse_and_check(code);

    // Should emit warning because one branch is dynamic
    assert!(checker.used_keys.is_empty());
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::VariableKey
    ));
}

#[test]
fn test_ternary_with_template_literals() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ approved }) {
              return <span>{t(approved ? `yes` : `no`)}</span>;
          }
      "#;
    let checker = parse_and_check(code);

    // Template literals without expressions should work (use HashSet)
    let keys: HashSet<_> = checker
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains("Common.yes"));
    assert!(keys.contains("Common.no"));
    assert!(checker.warnings.is_empty());
}

#[test]
fn test_ternary_with_parens() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ flag }) {
              t((flag) ? "a" : "b");
              t(flag ? ("c") : ("d"));
          }
      "#;
    let checker = parse_and_check(code);

    // All 4 keys should be extracted despite parentheses
    let keys: HashSet<_> = checker
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert_eq!(keys.len(), 4);
    assert!(keys.contains("Common.a"));
    assert!(keys.contains("Common.b"));
    assert!(keys.contains("Common.c"));
    assert!(keys.contains("Common.d"));
    assert!(checker.warnings.is_empty());
}

#[test]
fn test_ternary_template_with_expr_reason() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ flag, x }) {
              return <span>{t(flag ? `${x}` : "fallback")}</span>;
          }
      "#;
    let checker = parse_and_check(code);

    // Should emit TemplateWithExpr warning, not VariableKey
    assert!(checker.used_keys.is_empty());
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::TemplateWithExpr
    ));
}

// Helper for tests with custom Registries
fn parse_and_check_with_registries(
    code: &str,
    registries: &'static Registries,
) -> MissingKeyChecker<'static> {
    let source_map = Box::leak(Box::new(swc_common::SourceMap::default()));
    let source_file =
        source_map.new_source_file(FileName::Real("test.tsx".into()).into(), code.to_string());

    let syntax = Syntax::Typescript(TsSyntax {
        tsx: true,
        ..Default::default()
    });
    let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
    let module = parser.parse_module().unwrap();

    let file_path = Box::leak(Box::new("test.tsx".to_string()));
    let file_imports = Box::leak(Box::new(FileImports::new()));
    let available_keys = HashSet::new();
    let mut checker = MissingKeyChecker::new(
        file_path,
        source_map,
        registries,
        file_imports,
        code,
        &available_keys,
    );
    checker.visit_module(&module);
    checker
}

#[test]
fn test_resolvable_dynamic_key_local_object() {
    use crate::checkers::value_source::ValueSource;

    // Create registry with a local object
    let mut key_object = KeyObjectRegistry::new();
    key_object.insert(
        make_registry_key("test.tsx", "toolKeys"),
        KeyObject {
            name: "toolKeys".to_string(),
            file_path: "test.tsx".to_string(),
            is_exported: false,
            is_module_level: true,
            candidate_keys: vec!["createNovel".to_string(), "updateNovel".to_string()],
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_key_objects(key_object)));

    let code = r#"
        const toolKeys = { createNovel: "createNovel", updateNovel: "updateNovel" };
        const t = useTranslations("Tools");
        export function Tool({ toolName }) {
            const key = toolKeys[toolName];
            return t(key);
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    // Should have resolvable dynamic key, not warning
    assert!(checker.warnings.is_empty());

    // Find the resolved key for t(key)
    let resolved = checker
        .resolved_keys
        .iter()
        .find(|k| matches!(&k.source, ValueSource::ObjectAccess { .. }))
        .expect("Should have ObjectAccess resolved key");
    assert_eq!(resolved.namespace, Some("Tools".to_string()));
    if let ValueSource::ObjectAccess {
        object_name,
        candidate_values,
    } = &resolved.source
    {
        assert_eq!(object_name, "toolKeys");
        assert_eq!(
            candidate_values,
            &vec!["createNovel".to_string(), "updateNovel".to_string()]
        );
    } else {
        panic!("Expected ObjectAccess");
    }
}

#[test]
fn test_resolvable_dynamic_key_without_namespace() {
    use crate::checkers::value_source::ValueSource;

    let mut key_object = KeyObjectRegistry::new();
    key_object.insert(
        make_registry_key("test.tsx", "keys"),
        KeyObject {
            name: "keys".to_string(),
            file_path: "test.tsx".to_string(),
            is_exported: false,
            is_module_level: true,
            candidate_keys: vec!["key1".to_string()],
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_key_objects(key_object)));

    let code = r#"
        const keys = { a: "key1" };
        const t = useTranslations();
        export function Comp({ name }) {
            const k = keys[name];
            return t(k);
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    assert!(checker.warnings.is_empty());

    // Find the resolved key for t(k)
    let resolved = checker
        .resolved_keys
        .iter()
        .find(|k| matches!(&k.source, ValueSource::ObjectAccess { .. }))
        .expect("Should have ObjectAccess resolved key");
    assert_eq!(resolved.namespace, None);
}

#[test]
fn test_unresolvable_dynamic_key_unknown_object() {
    // Empty registry - object not found
    let registries = Box::leak(Box::new(create_empty_registries()));

    let code = r#"
        const unknownKeys = { a: "a" };
        const t = useTranslations("Common");
        export function Comp({ name }) {
            const key = unknownKeys[name];
            return t(key);
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    // Should emit warning since object is not in registry
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::VariableKey
    ));
}

#[test]
fn test_unresolvable_dynamic_key_no_object_access() {
    let registries = Box::leak(Box::new(create_empty_registries()));

    let code = r#"
        const t = useTranslations("Common");
        export function Comp({ keyFromProps }) {
            // key comes from props, not from object access
            return t(keyFromProps);
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    // Should emit warning since variable source is unknown
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::VariableKey
    ));
}

// Helper for tests with KeyArrayRegistry
fn create_registries_with_key_arrays(key_array: KeyArrayRegistry) -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object: KeyObjectRegistry::new(),
        key_array,
        string_array: StringArrayRegistry::new(),
    }
}

#[test]
fn test_resolvable_template_key_with_array() {
    use crate::checkers::value_source::ValueSource;

    // Create array registry
    let mut key_array = KeyArrayRegistry::new();
    let mut property_values = HashMap::new();
    property_values.insert(
        "titleKey".to_string(),
        vec![
            "novelManagement".to_string(),
            "characterDevelopment".to_string(),
        ],
    );
    key_array.insert(
        make_registry_key("test.tsx", "capabilities"),
        KeyArray {
            name: "capabilities".to_string(),
            file_path: "test.tsx".to_string(),
            is_exported: false,
            is_module_level: true,
            property_values,
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_key_arrays(key_array)));

    let code = r#"
        const capabilities = [
            { titleKey: "novelManagement" },
            { titleKey: "characterDevelopment" },
        ];
        const t = useTranslations("Ns");
        export function Comp() {
            return capabilities.map(cap => (
                <div>{t(`capabilities.${cap.titleKey}.title`)}</div>
            ));
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    // Should resolve template key, no warning
    assert!(checker.warnings.is_empty());

    // Find the resolved Template key
    let resolved = checker
        .resolved_keys
        .iter()
        .find(|k| matches!(&k.source, ValueSource::Template { .. }))
        .expect("Should have Template resolved key");

    if let ValueSource::Template {
        prefix,
        suffix,
        inner,
    } = &resolved.source
    {
        assert_eq!(prefix, "capabilities.");
        assert_eq!(suffix, ".title");
        if let ValueSource::ArrayIteration {
            property_name,
            candidate_values,
            ..
        } = inner.as_ref()
        {
            assert_eq!(property_name, "titleKey");
            assert_eq!(
                candidate_values,
                &vec![
                    "novelManagement".to_string(),
                    "characterDevelopment".to_string()
                ]
            );
        } else {
            panic!("Expected ArrayIteration inner");
        }
    } else {
        panic!("Expected Template");
    }
}

#[test]
fn test_template_with_untracked_iterator() {
    // Empty registries
    let registries = Box::leak(Box::new(create_empty_registries()));

    let code = r#"
        const t = useTranslations("Ns");
        export function Comp({ items }) {
            return items.map(item => (
                <div>{t(`prefix.${item.key}.suffix`)}</div>
            ));
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    // Should emit warning since array is not in registry
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::TemplateWithExpr
    ));
}

#[test]
fn test_template_with_multiple_expressions() {
    let registries = Box::leak(Box::new(create_empty_registries()));

    let code = r#"
        const t = useTranslations("Ns");
        export function Comp() {
            return t(`${prefix}.${suffix}`);
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    // Templates with multiple expressions should emit warning
    assert_eq!(checker.warnings.len(), 1);
    assert!(matches!(
        checker.warnings[0].reason,
        DynamicKeyReason::TemplateWithExpr
    ));
}

#[test]
fn test_iterator_source_tracking_foreach() {
    use crate::checkers::value_source::ValueSource;

    let mut key_array = KeyArrayRegistry::new();
    let mut property_values = HashMap::new();
    property_values.insert("key".to_string(), vec!["value1".to_string()]);
    key_array.insert(
        make_registry_key("test.tsx", "items"),
        KeyArray {
            name: "items".to_string(),
            file_path: "test.tsx".to_string(),
            is_exported: false,
            is_module_level: true,
            property_values,
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_key_arrays(key_array)));

    let code = r#"
        const items = [{ key: "value1" }];
        const t = useTranslations("Ns");
        export function Comp() {
            items.forEach(item => {
                console.log(t(`prefix.${item.key}.suffix`));
            });
        }
    "#;
    let checker = parse_and_check_with_registries(code, registries);

    assert!(checker.warnings.is_empty());
    // Check that we have a resolved Template key
    assert!(
        checker
            .resolved_keys
            .iter()
            .any(|k| matches!(&k.source, ValueSource::Template { .. }))
    );
}

// ============================================================
// ValueSource system tests
// ============================================================

mod value_source_tests {
    use super::*;
    use crate::checkers::key_objects::KeyObjectCollector;
    use swc_ecma_visit::VisitWith;

    /// Helper that collects all registries (objects, arrays, string arrays) from code
    /// before checking. This is needed for tests that rely on StringArrayRegistry.
    fn parse_and_check_with_collected_registries(code: &str) -> MissingKeyChecker<'static> {
        let source_map = Box::leak(Box::new(swc_common::SourceMap::default()));
        let source_file =
            source_map.new_source_file(FileName::Real("test.tsx".into()).into(), code.to_string());

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        });
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
        let module = parser.parse_module().unwrap();

        // Collect all registries from the code
        let file_path_str = "test.tsx";
        let mut collector = KeyObjectCollector::new(file_path_str);
        module.visit_with(&mut collector);

        // Build registries
        let mut key_object = KeyObjectRegistry::new();
        let mut key_array = KeyArrayRegistry::new();
        let mut string_array = StringArrayRegistry::new();

        for obj in collector.objects {
            let key = make_registry_key(&obj.file_path, &obj.name);
            key_object.insert(key, obj);
        }
        for arr in collector.arrays {
            let key = make_registry_key(&arr.file_path, &arr.name);
            key_array.insert(key, arr);
        }
        for str_arr in collector.string_arrays {
            let key = make_registry_key(&str_arr.file_path, &str_arr.name);
            string_array.insert(key, str_arr);
        }

        // Create Registries and leak for 'static lifetime
        let file_path = Box::leak(Box::new(file_path_str.to_string()));
        let registries = Box::leak(Box::new(Registries {
            schema: SchemaRegistry::new(),
            key_object,
            key_array,
            string_array,
        }));
        let file_imports = Box::leak(Box::new(collector.imports));

        let available_keys = HashSet::new();
        let mut checker = MissingKeyChecker::new(
            file_path,
            source_map,
            registries,
            file_imports,
            code,
            &available_keys,
        );
        checker.visit_module(&module);
        checker
    }

    // Case 1: String array iteration
    #[test]
    fn test_string_array_iteration_feature_keys() {
        let code = r#"
            const FEATURE_KEYS = ["save", "characters", "chapters", "ai"] as const;
            const t = useTranslations("Ns");
            export function Comp() {
                return FEATURE_KEYS.map((featureKey) => (
                    <li key={featureKey}>{t(`features.${featureKey}`)}</li>
                ));
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // No warnings - keys are resolvable through StringArrayRegistry
        assert!(checker.warnings.is_empty());

        // Check resolved_keys contains the right ValueSource
        assert_eq!(checker.resolved_keys.len(), 1);
        let resolved = &checker.resolved_keys[0];
        assert_eq!(resolved.namespace, Some("Ns".to_string()));

        // Verify it resolves to the expected keys
        let keys = resolved.source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 4);
        assert!(keys.contains(&"features.save".to_string()));
        assert!(keys.contains(&"features.characters".to_string()));
        assert!(keys.contains(&"features.chapters".to_string()));
        assert!(keys.contains(&"features.ai".to_string()));
    }

    // Case 2: String array iteration - SUPPORTED_GENDERS
    #[test]
    fn test_string_array_iteration_genders() {
        let code = r#"
            const SUPPORTED_GENDERS = ["male", "female", "other"] as const;
            const t = useTranslations("Form");
            export function Comp() {
                return SUPPORTED_GENDERS.map((gender) => (
                    <option key={gender}>{t(`genderOptions.${gender}`)}</option>
                ));
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // No warnings - keys are resolvable
        assert!(checker.warnings.is_empty());

        // Check resolved keys
        assert_eq!(checker.resolved_keys.len(), 1);
        let keys = checker.resolved_keys[0].source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"genderOptions.male".to_string()));
        assert!(keys.contains(&"genderOptions.female".to_string()));
        assert!(keys.contains(&"genderOptions.other".to_string()));
    }

    // Case 3: Parameter property access - unresolvable
    #[test]
    fn test_param_property_unresolvable() {
        let code = r#"
            const t = useTranslations("Card");
            export function Component({ character }: { character: Character }) {
                return <span>{t(`genderOptions.${character.gender}`)}</span>;
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // Warning expected - character is a function parameter, unresolvable
        assert_eq!(checker.warnings.len(), 1);
        assert!(matches!(
            checker.warnings[0].reason,
            DynamicKeyReason::TemplateWithExpr
        ));

        // Check resolved_keys shows Unresolvable
        assert_eq!(checker.resolved_keys.len(), 1);
        assert!(checker.resolved_keys[0].source.resolve_keys().is_err());
    }

    // Case 4: Function return value property - unresolvable
    #[test]
    fn test_function_return_unresolvable() {
        let code = r#"
            const t = useTranslations("Upload");
            export function Comp() {
                const result = validateFile(file);
                return <span>{t(`error.${result.error}`)}</span>;
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // Warning expected - result is a function call result, unresolvable
        assert_eq!(checker.warnings.len(), 1);
    }

    // Case 5: Conditional with variable - unresolvable
    #[test]
    fn test_conditional_with_variable_unresolvable() {
        let code = r#"
            const t = useTranslations("Wizard");
            export function Comp() {
                const [currentStep, setCurrentStep] = useState(0);
                return <h1>{t(`step${currentStep === 4 ? 5 : currentStep}.title`)}</h1>;
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // Warning expected - currentStep is a variable from useState, unresolvable
        assert_eq!(checker.warnings.len(), 1);
    }

    // Case 6: Static string - basic case
    #[test]
    fn test_static_string_literal() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp() {
                return <span>{t("static.key")}</span>;
            }
        "#;
        let checker = parse_and_check(code);

        // Static string should be extracted as UsedKey
        assert!(checker.warnings.is_empty());
        assert_eq!(checker.used_keys.len(), 1);
        assert_eq!(checker.used_keys[0].full_key, "Ns.static.key");
    }

    // Case 7: Simple conditional - both sides are literals
    #[test]
    fn test_simple_conditional_both_literals() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp({ condition }) {
                return <span>{t(condition ? "keyA" : "keyB")}</span>;
            }
        "#;
        let checker = parse_and_check(code);

        // Both sides are literals - extract both keys
        assert!(checker.warnings.is_empty());
        assert_eq!(checker.used_keys.len(), 2);

        let keys: HashSet<_> = checker
            .used_keys
            .iter()
            .map(|k| k.full_key.as_str())
            .collect();
        assert!(keys.contains("Ns.keyA"));
        assert!(keys.contains("Ns.keyB"));
    }

    // Case 10: Nested combination - string array + conditional
    #[test]
    fn test_nested_string_array_conditional() {
        let code = r#"
            const KEYS = ["a", "b"] as const;
            const t = useTranslations("Ns");
            export function Comp({ flag }) {
                return KEYS.map(k => (
                    <span key={k}>{t(flag ? `${k}.plural` : `${k}.singular`)}</span>
                ));
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // No warnings - all branches are resolvable
        assert!(checker.warnings.is_empty());

        // Check resolved_keys
        assert_eq!(checker.resolved_keys.len(), 1);
        let keys = checker.resolved_keys[0].source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 4);
        assert!(keys.contains(&"a.plural".to_string()));
        assert!(keys.contains(&"b.plural".to_string()));
        assert!(keys.contains(&"a.singular".to_string()));
        assert!(keys.contains(&"b.singular".to_string()));
    }

    // Case 11: Logical OR expression
    #[test]
    fn test_logical_or_expression() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp({ key }) {
                return <span>{t(key || "fallback")}</span>;
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // Warning expected - left side is a variable, unresolvable
        assert_eq!(checker.warnings.len(), 1);
    }

    // Case 12: Multi-expression template - unresolvable
    #[test]
    fn test_multi_expr_template_unresolvable() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp({ prefix, suffix }) {
                return <span>{t(`${prefix}.${suffix}`)}</span>;
            }
        "#;
        let checker = parse_and_check(code);

        // Multi-expression templates should emit warning
        assert_eq!(checker.warnings.len(), 1);
        assert!(matches!(
            checker.warnings[0].reason,
            DynamicKeyReason::TemplateWithExpr
        ));
    }

    // Case 15: forEach with string array iteration
    #[test]
    fn test_foreach_string_array_iteration() {
        let code = r#"
            const KEYS = ["a", "b"] as const;
            const t = useTranslations("Ns");
            export function Comp() {
                KEYS.forEach(k => {
                    console.log(t(`prefix.${k}`));
                });
                return null;
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // forEach should work like map - iterator is recognized
        assert!(checker.warnings.is_empty());

        // Check resolved_keys
        assert_eq!(checker.resolved_keys.len(), 1);
        let keys = checker.resolved_keys[0].source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"prefix.a".to_string()));
        assert!(keys.contains(&"prefix.b".to_string()));
    }

    // Case 16: Nested conditional expression
    #[test]
    fn test_nested_conditional_expression() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp({ flag1, flag2 }) {
                return <span>{t(flag1 ? (flag2 ? "keyA" : "keyB") : "keyC")}</span>;
            }
        "#;
        let checker = parse_and_check(code);

        // All branches are string literals - should resolve without warnings
        assert!(checker.warnings.is_empty());
        assert_eq!(checker.resolved_keys.len(), 1);

        let keys = checker.resolved_keys[0].source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"keyA".to_string()));
        assert!(keys.contains(&"keyB".to_string()));
        assert!(keys.contains(&"keyC".to_string()));
    }

    // Case 17: Logical AND expression (short-circuit)
    #[test]
    fn test_logical_and_expression() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp({ isEnabled }) {
                return <span>{t(isEnabled && "feature.enabled")}</span>;
            }
        "#;
        let checker = parse_and_check(code);

        // Logical AND is not currently supported for key resolution
        // Left side is a variable, should emit a warning
        assert_eq!(checker.warnings.len(), 1);
    }

    #[test]
    fn test_nested_iterators_with_same_param_name() {
        // Test that nested iterators with the same param name don't conflict
        let code = r#"
            const OUTER = [{ key: "outer1" }, { key: "outer2" }];
            const INNER = [{ key: "inner1" }, { key: "inner2" }];
            const t = useTranslations("Ns");
            export function Comp() {
                return OUTER.map(item => (
                    <div>
                        {t(`outer.${item.key}`)}
                        {INNER.map(item => t(`inner.${item.key}`))}
                    </div>
                ));
            }
        "#;

        let checker = parse_and_check_with_collected_registries(code);

        // Should have 2 resolved keys: one from OUTER and one from INNER
        assert_eq!(checker.resolved_keys.len(), 2);

        // Both should resolve successfully (no unresolvable warnings)
        for key in &checker.resolved_keys {
            let result = key.source.resolve_keys();
            assert!(result.is_ok(), "Key should resolve, but got: {:?}", result);
        }

        // Check the resolved values
        let all_keys: Vec<String> = checker
            .resolved_keys
            .iter()
            .flat_map(|k| k.source.resolve_keys().unwrap())
            .collect();

        // Should contain outer keys with "outer." prefix
        assert!(all_keys.iter().any(|k| k.starts_with("outer.")));
        // Should contain inner keys with "inner." prefix
        assert!(all_keys.iter().any(|k| k.starts_with("inner.")));

        // Verify specific keys
        assert!(all_keys.contains(&"outer.outer1".to_string()));
        assert!(all_keys.contains(&"outer.outer2".to_string()));
        assert!(all_keys.contains(&"inner.inner1".to_string()));
        assert!(all_keys.contains(&"inner.inner2".to_string()));
    }

    #[test]
    fn test_sibling_iterators_with_same_param_name() {
        // Test that sibling (not nested) iterators with the same param name
        // correctly resolve to their respective arrays
        let code = r#"
            const ARRAY_A = [{ key: "a1" }, { key: "a2" }];
            const ARRAY_B = [{ key: "b1" }, { key: "b2" }];
            const t = useTranslations("Ns");
            export function Comp() {
                return (
                    <>
                        {ARRAY_A.map(item => <span>{t(`a.${item.key}`)}</span>)}
                        {ARRAY_B.map(item => <span>{t(`b.${item.key}`)}</span>)}
                    </>
                );
            }
        "#;

        let checker = parse_and_check_with_collected_registries(code);

        // Should have 2 resolved keys
        assert_eq!(checker.resolved_keys.len(), 2);

        // Check the resolved values
        let all_keys: Vec<String> = checker
            .resolved_keys
            .iter()
            .flat_map(|k| k.source.resolve_keys().unwrap())
            .collect();

        // Verify specific keys from both arrays
        assert!(all_keys.contains(&"a.a1".to_string()));
        assert!(all_keys.contains(&"a.a2".to_string()));
        assert!(all_keys.contains(&"b.b1".to_string()));
        assert!(all_keys.contains(&"b.b2".to_string()));
    }

    // Case: Function-scoped object should be resolvable
    #[test]
    fn test_function_scoped_object_resolvable() {
        let code = r#"
            import { useTranslations } from "next-intl";

            export function useToolDisplayName(toolName: string): string {
                const t = useTranslations("ToolApprovals.tools");
                const toolKeys: Record<string, string> = {
                    createNovel: "createNovel",
                    updateNovel: "updateNovel",
                };
                const key = toolKeys[toolName];
                if (key) {
                    return t(key);
                }
                return toolName;
            }
        "#;
        let checker = parse_and_check_with_collected_registries(code);

        // Should NOT emit dynamic key warning - the object is in the same file
        assert!(
            checker.warnings.is_empty(),
            "Function-scoped object should be resolvable, got warnings: {:?}",
            checker.warnings
        );

        // Check resolved_keys contains the object access
        let resolved = checker
            .resolved_keys
            .iter()
            .find(|k| {
                matches!(
                    &k.source,
                    crate::checkers::value_source::ValueSource::ObjectAccess { .. }
                )
            })
            .expect("Should have ObjectAccess resolved key");

        // Verify namespace
        assert_eq!(resolved.namespace, Some("ToolApprovals.tools".to_string()));

        // Verify candidate keys
        let keys = resolved.source.resolve_keys().unwrap();
        assert!(keys.contains(&"createNovel".to_string()));
        assert!(keys.contains(&"updateNovel".to_string()));
    }
}

#[test]
fn test_t_raw_method_call() {
    // Test that t.raw("key") is detected as a translation key usage
    let code = r#"
        const t = useTranslations("HomePage");
        const benefits = t.raw("creationMethods.benefits") as string[];
    "#;
    let checker = parse_and_check(code);

    assert_eq!(
        checker.used_keys.len(),
        1,
        "t.raw() should be detected as a used key"
    );
    assert_eq!(
        checker.used_keys[0].full_key,
        "HomePage.creationMethods.benefits"
    );
}

#[test]
fn test_t_rich_method_call() {
    // Test that t.rich("key", {...}) is detected as a translation key usage
    let code = r#"
        const t = useTranslations("Common");
        const content = t.rich("welcome", { bold: (text) => <b>{text}</b> });
    "#;
    let checker = parse_and_check(code);

    assert_eq!(
        checker.used_keys.len(),
        1,
        "t.rich() should be detected as a used key"
    );
    assert_eq!(checker.used_keys[0].full_key, "Common.welcome");
}

#[test]
fn test_t_markup_method_call() {
    // Test that t.markup("key") is detected as a translation key usage
    let code = r#"
        const t = useTranslations("Common");
        const html = t.markup("richText");
    "#;
    let checker = parse_and_check(code);

    assert_eq!(
        checker.used_keys.len(),
        1,
        "t.markup() should be detected as a used key"
    );
    assert_eq!(checker.used_keys[0].full_key, "Common.richText");
}

#[test]
fn test_t_raw_with_await_get_translations() {
    // Test that async getTranslations + t.raw() works
    let code = r#"
        const t = await getTranslations("HomePage");
        const benefits = t.raw("features.list") as string[];
    "#;
    let checker = parse_and_check(code);

    assert_eq!(
        checker.used_keys.len(),
        1,
        "t.raw() with async getTranslations should be detected"
    );
    assert_eq!(checker.used_keys[0].full_key, "HomePage.features.list");
}

#[test]
fn test_t_raw_without_namespace() {
    // Test t.raw() when useTranslations has no namespace
    let code = r#"
        const t = useTranslations();
        const items = t.raw("Page.items") as string[];
    "#;
    let checker = parse_and_check(code);

    assert_eq!(
        checker.used_keys.len(),
        1,
        "t.raw() without namespace should be detected"
    );
    // Without namespace, key should be used as-is
    assert_eq!(checker.used_keys[0].full_key, "Page.items");
}

#[test]
fn test_unrelated_raw_method_not_detected() {
    // Test that .raw() on non-translation objects is NOT detected
    let code = r#"
        const t = useTranslations("Common");
        const someObj = { raw: (key) => key };
        const result = someObj.raw("notATranslationKey");
    "#;
    let checker = parse_and_check(code);

    // Only t("...") calls should be detected, not someObj.raw()
    assert!(
        checker.used_keys.is_empty(),
        "Non-translation .raw() should not be detected as used key"
    );
}

#[test]
fn test_t_raw_with_template_literal() {
    // Test t.raw() with template literal (no expressions)
    let code = r#"
        const t = useTranslations("Page");
        const items = t.raw(`benefits.list`);
    "#;
    let checker = parse_and_check(code);

    assert_eq!(
        checker.used_keys.len(),
        1,
        "t.raw() with template literal should be detected"
    );
    assert_eq!(checker.used_keys[0].full_key, "Page.benefits.list");
}

#[test]
fn test_t_raw_with_dynamic_key_not_detected() {
    // Test that t.raw(variable) is NOT detected as a used key
    // (consistent with t(variable) behavior)
    let code = r#"
        const t = useTranslations("Common");
        const key = "dynamic.key";
        const result = t.raw(key);
    "#;
    let checker = parse_and_check(code);

    // Dynamic keys should not be reported as used_keys
    assert!(
        checker.used_keys.is_empty(),
        "t.raw() with dynamic key should not be detected as used key"
    );
}
