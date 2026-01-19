//! Tests for translation key extraction.

use std::collections::{HashMap, HashSet};
use swc_common::FileName;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};
use swc_ecma_visit::Visit;

use super::*;
use crate::checkers::key_objects::{
    FileImports, KeyArrayRegistry, KeyObject, KeyObjectRegistry, StringArrayRegistry,
    TranslationFnCall, TranslationFnCallRegistry, TranslationProp, TranslationPropRegistry,
    make_registry_key, make_translation_fn_call_key, make_translation_prop_key,
};
use crate::checkers::schema::SchemaRegistry;
use crate::commands::context::Registries;

fn create_empty_registries() -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object: KeyObjectRegistry::new(),
        key_array: KeyArrayRegistry::new(),
        string_array: StringArrayRegistry::new(),
        translation_prop: TranslationPropRegistry::new(),
        translation_fn_call: TranslationFnCallRegistry::new(),
        default_exports: HashMap::new(),
    }
}

fn create_registries_with_key_objects(key_object: KeyObjectRegistry) -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object,
        key_array: KeyArrayRegistry::new(),
        string_array: StringArrayRegistry::new(),
        translation_prop: TranslationPropRegistry::new(),
        translation_fn_call: TranslationFnCallRegistry::new(),
        default_exports: HashMap::new(),
    }
}

fn parse_and_extract(code: &str) -> TranslationKeyVisitor<'static> {
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
    let available_keys = Box::leak(Box::new(HashSet::new()));
    let mut visitor = TranslationKeyVisitor::new(
        file_path,
        source_map,
        registries,
        file_imports,
        code,
        available_keys,
    );
    visitor.visit_module(&module);
    visitor
}

#[test]
fn test_use_translations_with_namespace() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button() {
              return <button>{t("submit")}</button>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Common.submit");
}

#[test]
fn test_use_translations_without_namespace() {
    let code = r#"
          const t = useTranslations();
          export function Button() {
              return <button>{t("Common.submit")}</button>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Common.submit");
}

#[test]
fn test_await_get_translations() {
    let code = r#"
          const t = await getTranslations("Auth");
          export function Page() {
              return <div>{t("login")}</div>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Auth.login");
}

#[test]
fn test_renamed_function() {
    let code = r#"
          const translate = useTranslations("Common");
          export function Button() {
              return <button>{translate("submit")}</button>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Common.submit");
}

#[test]
fn test_dynamic_key_warning() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button({ keyName }) {
              return <button>{t(keyName)}</button>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert!(visitor.used_keys.is_empty());
    assert_eq!(visitor.warnings.len(), 1);
    assert!(matches!(
        visitor.warnings[0].reason,
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
    let visitor = parse_and_extract(code);

    assert!(visitor.used_keys.is_empty());
    assert_eq!(visitor.warnings.len(), 1);
    assert!(matches!(
        visitor.warnings[0].reason,
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
    let visitor = parse_and_extract(code);
    assert_eq!(visitor.warnings.len(), 1);
    let hint = visitor.warnings[0].hint.as_ref().unwrap();
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
    let visitor = parse_and_extract(code);
    assert_eq!(visitor.warnings.len(), 1);
    let hint = visitor.warnings[0].hint.as_ref().unwrap();
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
    let visitor = parse_and_extract(code);
    assert_eq!(visitor.warnings.len(), 1);
    let hint = visitor.warnings[0].hint.as_ref().unwrap();
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
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 2);
    assert_eq!(visitor.used_keys[0].full_key, "Common.submit");
    assert_eq!(visitor.used_keys[1].full_key, "Common.cancel");
}

#[test]
fn test_key_with_dot_prepends_namespace() {
    let code = r#"
          const t = useTranslations("Common");
          export function Button() {
              return <button>{t("Button.submit")}</button>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Common.Button.submit");
}

#[test]
fn test_ternary_with_string_literals() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ approved }) {
              return <span>{t(approved ? "approved" : "rejected")}</span>;
          }
      "#;
    let visitor = parse_and_extract(code);

    let keys: HashSet<_> = visitor
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains("Common.approved"));
    assert!(keys.contains("Common.rejected"));
    assert!(visitor.warnings.is_empty());
}

#[test]
fn test_ternary_with_mixed_branches() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ key, flag }) {
              return <span>{t(flag ? key : "fallback")}</span>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert!(visitor.used_keys.is_empty());
    assert_eq!(visitor.warnings.len(), 1);
    assert!(matches!(
        visitor.warnings[0].reason,
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
    let visitor = parse_and_extract(code);

    let keys: HashSet<_> = visitor
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains("Common.yes"));
    assert!(keys.contains("Common.no"));
    assert!(visitor.warnings.is_empty());
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
    let visitor = parse_and_extract(code);

    let keys: HashSet<_> = visitor
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert_eq!(keys.len(), 4);
    assert!(keys.contains("Common.a"));
    assert!(keys.contains("Common.b"));
    assert!(keys.contains("Common.c"));
    assert!(keys.contains("Common.d"));
    assert!(visitor.warnings.is_empty());
}

#[test]
fn test_ternary_template_with_expr_reason() {
    let code = r#"
          const t = useTranslations("Common");
          export function Status({ flag, x }) {
              return <span>{t(flag ? `${x}` : "fallback")}</span>;
          }
      "#;
    let visitor = parse_and_extract(code);

    assert!(visitor.used_keys.is_empty());
    assert_eq!(visitor.warnings.len(), 1);
    assert!(matches!(
        visitor.warnings[0].reason,
        DynamicKeyReason::TemplateWithExpr
    ));
}

// Helper for tests with custom Registries
fn parse_and_extract_with_registries(
    code: &str,
    registries: &'static Registries,
) -> TranslationKeyVisitor<'static> {
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
    let available_keys = Box::leak(Box::new(HashSet::new()));
    let mut visitor = TranslationKeyVisitor::new(
        file_path,
        source_map,
        registries,
        file_imports,
        code,
        available_keys,
    );
    visitor.visit_module(&module);
    visitor
}

#[test]
fn test_resolvable_dynamic_key_local_object() {
    use crate::checkers::value_source::ValueSource;

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
    let visitor = parse_and_extract_with_registries(code, registries);

    assert!(visitor.warnings.is_empty());

    let resolved = visitor
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
fn test_t_raw_method_call() {
    let code = r#"
        const t = useTranslations("HomePage");
        const benefits = t.raw("creationMethods.benefits") as string[];
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(
        visitor.used_keys.len(),
        1,
        "t.raw() should be detected as a used key"
    );
    assert_eq!(
        visitor.used_keys[0].full_key,
        "HomePage.creationMethods.benefits"
    );
}

#[test]
fn test_t_rich_method_call() {
    let code = r#"
        const t = useTranslations("Common");
        const content = t.rich("welcome", { bold: (text) => <b>{text}</b> });
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(
        visitor.used_keys.len(),
        1,
        "t.rich() should be detected as a used key"
    );
    assert_eq!(visitor.used_keys[0].full_key, "Common.welcome");
}

#[test]
fn test_t_markup_method_call() {
    let code = r#"
        const t = useTranslations("Common");
        const html = t.markup("richText");
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(
        visitor.used_keys.len(),
        1,
        "t.markup() should be detected as a used key"
    );
    assert_eq!(visitor.used_keys[0].full_key, "Common.richText");
}

// ============================================================
// Translation Props Tests
// ============================================================

fn create_registries_with_translation_props(
    translation_prop: TranslationPropRegistry,
) -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object: KeyObjectRegistry::new(),
        key_array: KeyArrayRegistry::new(),
        string_array: StringArrayRegistry::new(),
        translation_prop,
        translation_fn_call: TranslationFnCallRegistry::new(),
        default_exports: HashMap::new(),
    }
}

#[test]
fn test_translation_prop_function_declaration() {
    let mut translation_prop = TranslationPropRegistry::new();
    translation_prop.insert(
        make_translation_prop_key("AdultLandingPage", "t"),
        TranslationProp {
            component_name: "AdultLandingPage".to_string(),
            prop_name: "t".to_string(),
            namespaces: vec![Some("NSFWAIStoryGenerator".to_string())],
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_translation_props(
        translation_prop,
    )));

    let code = r#"
        function AdultLandingPage({ t }: Props) {
            return <div>{t("features.title")}</div>;
        }
    "#;

    let visitor = parse_and_extract_with_registries(code, registries);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(
        visitor.used_keys[0].full_key,
        "NSFWAIStoryGenerator.features.title"
    );
}

#[test]
fn test_translation_prop_arrow_function() {
    let mut translation_prop = TranslationPropRegistry::new();
    translation_prop.insert(
        make_translation_prop_key("MyComponent", "translate"),
        TranslationProp {
            component_name: "MyComponent".to_string(),
            prop_name: "translate".to_string(),
            namespaces: vec![Some("Dashboard".to_string())],
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_translation_props(
        translation_prop,
    )));

    let code = r#"
        const MyComponent = ({ translate }: Props) => {
            return <span>{translate("welcome")}</span>;
        };
    "#;

    let visitor = parse_and_extract_with_registries(code, registries);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Dashboard.welcome");
}

// ============================================================
// Translation Function Call Tests
// ============================================================

fn create_registries_with_translation_fn_calls(
    translation_fn_call: TranslationFnCallRegistry,
) -> Registries {
    Registries {
        schema: SchemaRegistry::new(),
        key_object: KeyObjectRegistry::new(),
        key_array: KeyArrayRegistry::new(),
        string_array: StringArrayRegistry::new(),
        translation_prop: TranslationPropRegistry::new(),
        translation_fn_call,
        default_exports: HashMap::new(),
    }
}

#[test]
fn test_translation_fn_call_arrow_function() {
    let mut translation_fn_call = TranslationFnCallRegistry::new();
    translation_fn_call.insert(
        make_translation_fn_call_key("test.tsx", "usageTypeLabels", 0),
        TranslationFnCall {
            fn_file_path: "test.tsx".to_string(),
            fn_name: "usageTypeLabels".to_string(),
            arg_index: 0,
            namespaces: vec![Some("CreditsUsageList".to_string())],
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_translation_fn_calls(
        translation_fn_call,
    )));

    let code = r#"
        const usageTypeLabels = (t) => ({
            ai_chapter_generation: t("usageTypes.ai_chapter_generation"),
            ai_completion_plan: t("usageTypes.ai_completion_plan"),
        });
    "#;

    let visitor = parse_and_extract_with_registries(code, registries);

    assert_eq!(visitor.used_keys.len(), 2);
    let keys: HashSet<&str> = visitor
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert!(keys.contains("CreditsUsageList.usageTypes.ai_chapter_generation"));
    assert!(keys.contains("CreditsUsageList.usageTypes.ai_completion_plan"));
}

#[test]
fn test_translation_fn_call_function_declaration() {
    let mut translation_fn_call = TranslationFnCallRegistry::new();
    translation_fn_call.insert(
        make_translation_fn_call_key("test.tsx", "buildLabels", 0),
        TranslationFnCall {
            fn_file_path: "test.tsx".to_string(),
            fn_name: "buildLabels".to_string(),
            arg_index: 0,
            namespaces: vec![Some("MyNamespace".to_string())],
        },
    );
    let registries = Box::leak(Box::new(create_registries_with_translation_fn_calls(
        translation_fn_call,
    )));

    let code = r#"
        function buildLabels(t) {
            return {
                submit: t("submit"),
                cancel: t("cancel"),
            };
        }
    "#;

    let visitor = parse_and_extract_with_registries(code, registries);

    assert_eq!(visitor.used_keys.len(), 2);
    let keys: HashSet<&str> = visitor
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert!(keys.contains("MyNamespace.submit"));
    assert!(keys.contains("MyNamespace.cancel"));
}

#[test]
fn test_nested_function_binding_shadowing() {
    let code = r#"
        const t = useTranslations("Outer");

        function Parent() {
            return <div>{t("outerKey")}</div>;
        }

        function Child() {
            const t = useTranslations("Inner");
            return <div>{t("innerKey")}</div>;
        }
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 2);
    assert!(
        visitor
            .used_keys
            .iter()
            .any(|k| k.full_key == "Outer.outerKey")
    );
    assert!(
        visitor
            .used_keys
            .iter()
            .any(|k| k.full_key == "Inner.innerKey")
    );
}

#[test]
fn test_translation_binding_does_not_leak_between_siblings() {
    let code = r#"
        function ComponentA() {
            const t = useTranslations("A");
            return <div>{t("keyA")}</div>;
        }

        function ComponentB() {
            return <div>{t("keyB")}</div>;
        }
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "A.keyA");
}

// ============================================================
// Relative glot-message-keys Tests
// ============================================================

#[test]
fn test_relative_pattern_with_direct_namespace() {
    let code = r#"
        const t = useTranslations("Features");
        // glot-message-keys ".items.*.title"
        const title = t(`items.${key}.title`);
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "Features.items.*.title");
}

#[test]
fn test_relative_pattern_without_namespace() {
    let code = r#"
        const t = useTranslations();
        // glot-message-keys ".items.title"
        const title = t(`items.${key}.title`);
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 1);
    assert_eq!(visitor.used_keys[0].full_key, "items.title");
}

#[test]
fn test_mixed_absolute_and_relative_patterns() {
    let code = r#"
        const t = useTranslations("Page");
        // glot-message-keys "Common.button", ".features.title"
        const label = t(dynamic);
    "#;
    let visitor = parse_and_extract(code);

    assert_eq!(visitor.used_keys.len(), 2);
    let keys: Vec<&str> = visitor
        .used_keys
        .iter()
        .map(|k| k.full_key.as_str())
        .collect();
    assert!(keys.contains(&"Common.button"));
    assert!(keys.contains(&"Page.features.title"));
}

// ============================================================
// ValueSource Tests
// ============================================================

mod value_source_tests {
    use super::*;
    use crate::checkers::key_objects::KeyObjectCollector;
    use swc_ecma_visit::VisitWith;

    fn parse_and_extract_with_collected_registries(code: &str) -> TranslationKeyVisitor<'static> {
        let source_map = Box::leak(Box::new(swc_common::SourceMap::default()));
        let source_file =
            source_map.new_source_file(FileName::Real("test.tsx".into()).into(), code.to_string());

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        });
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
        let module = parser.parse_module().unwrap();

        let file_path_str = "test.tsx";
        let mut collector = KeyObjectCollector::new(file_path_str);
        module.visit_with(&mut collector);

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

        let file_path = Box::leak(Box::new(file_path_str.to_string()));
        let registries = Box::leak(Box::new(Registries {
            schema: SchemaRegistry::new(),
            key_object,
            key_array,
            string_array,
            translation_prop: TranslationPropRegistry::new(),
            translation_fn_call: TranslationFnCallRegistry::new(),
            default_exports: HashMap::new(),
        }));
        let file_imports = Box::leak(Box::new(collector.imports));

        let available_keys = Box::leak(Box::new(HashSet::new()));
        let mut visitor = TranslationKeyVisitor::new(
            file_path,
            source_map,
            registries,
            file_imports,
            code,
            available_keys,
        );
        visitor.visit_module(&module);
        visitor
    }

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
        let visitor = parse_and_extract_with_collected_registries(code);

        assert!(visitor.warnings.is_empty());
        assert_eq!(visitor.resolved_keys.len(), 1);
        let resolved = &visitor.resolved_keys[0];
        assert_eq!(resolved.namespace, Some("Ns".to_string()));

        let keys = resolved.source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 4);
        assert!(keys.contains(&"features.save".to_string()));
        assert!(keys.contains(&"features.characters".to_string()));
        assert!(keys.contains(&"features.chapters".to_string()));
        assert!(keys.contains(&"features.ai".to_string()));
    }

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
        let visitor = parse_and_extract_with_collected_registries(code);

        assert!(visitor.warnings.is_empty());
        assert_eq!(visitor.resolved_keys.len(), 1);
        let keys = visitor.resolved_keys[0].source.resolve_keys().unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"genderOptions.male".to_string()));
        assert!(keys.contains(&"genderOptions.female".to_string()));
        assert!(keys.contains(&"genderOptions.other".to_string()));
    }

    #[test]
    fn test_param_property_unresolvable() {
        let code = r#"
            const t = useTranslations("Card");
            export function Component({ character }: { character: Character }) {
                return <span>{t(`genderOptions.${character.gender}`)}</span>;
            }
        "#;
        let visitor = parse_and_extract_with_collected_registries(code);

        assert_eq!(visitor.warnings.len(), 1);
        assert!(matches!(
            visitor.warnings[0].reason,
            DynamicKeyReason::TemplateWithExpr
        ));

        assert_eq!(visitor.resolved_keys.len(), 1);
        assert!(visitor.resolved_keys[0].source.resolve_keys().is_err());
    }

    #[test]
    fn test_static_string_literal() {
        let code = r#"
            const t = useTranslations("Ns");
            export function Comp() {
                return <span>{t("static.key")}</span>;
            }
        "#;
        let visitor = parse_and_extract(code);

        assert!(visitor.warnings.is_empty());
        assert_eq!(visitor.used_keys.len(), 1);
        assert_eq!(visitor.used_keys[0].full_key, "Ns.static.key");
    }

    #[test]
    fn test_nested_iterators_with_same_param_name() {
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

        let visitor = parse_and_extract_with_collected_registries(code);

        assert_eq!(visitor.resolved_keys.len(), 2);

        for key in &visitor.resolved_keys {
            let result = key.source.resolve_keys();
            assert!(result.is_ok(), "Key should resolve, but got: {:?}", result);
        }

        let all_keys: Vec<String> = visitor
            .resolved_keys
            .iter()
            .flat_map(|k| k.source.resolve_keys().unwrap())
            .collect();

        assert!(all_keys.iter().any(|k| k.starts_with("outer.")));
        assert!(all_keys.iter().any(|k| k.starts_with("inner.")));

        assert!(all_keys.contains(&"outer.outer1".to_string()));
        assert!(all_keys.contains(&"outer.outer2".to_string()));
        assert!(all_keys.contains(&"inner.inner1".to_string()));
        assert!(all_keys.contains(&"inner.inner2".to_string()));
    }

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
        let visitor = parse_and_extract_with_collected_registries(code);

        assert!(
            visitor.warnings.is_empty(),
            "Function-scoped object should be resolvable, got warnings: {:?}",
            visitor.warnings
        );

        let resolved = visitor
            .resolved_keys
            .iter()
            .find(|k| {
                matches!(
                    &k.source,
                    crate::checkers::value_source::ValueSource::ObjectAccess { .. }
                )
            })
            .expect("Should have ObjectAccess resolved key");

        assert_eq!(resolved.namespace, Some("ToolApprovals.tools".to_string()));

        let keys = resolved.source.resolve_keys().unwrap();
        assert!(keys.contains(&"createNovel".to_string()));
        assert!(keys.contains(&"updateNovel".to_string()));
    }
}
