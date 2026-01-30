use glot::mcp::{
    GlotMcpServer,
    types::{AddTranslationsParams, TranslationEntry},
};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Map, json};

use crate::{McpTestFixture, extract_tool_result_json};

#[tokio::test]
async fn test_add_translations_single_locale_single_key() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("greeting".to_string(), json!("Hello"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "en".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalLocales"], 1);
    assert_eq!(json_result["summary"]["successfulLocales"], 1);
    assert_eq!(json_result["summary"]["failedLocales"], 0);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 1);
    assert_eq!(json_result["summary"]["totalKeysUpdated"], 0);

    // Verify file was created
    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(locale_data, json!({"greeting": "Hello"}));
}

#[tokio::test]
async fn test_add_translations_single_locale_multiple_keys() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("greeting".to_string(), json!("Hello"));
    keys.insert("farewell".to_string(), json!("Goodbye"));
    keys.insert("question".to_string(), json!("How are you?"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "en".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 3);

    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(locale_data["greeting"], "Hello");
    assert_eq!(locale_data["farewell"], "Goodbye");
    assert_eq!(locale_data["question"], "How are you?");
}

#[tokio::test]
async fn test_add_translations_multiple_locales() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut en_keys = Map::new();
    en_keys.insert("greeting".to_string(), json!("Hello"));

    let mut zh_keys = Map::new();
    zh_keys.insert("greeting".to_string(), json!("你好"));

    let mut ja_keys = Map::new();
    ja_keys.insert("greeting".to_string(), json!("こんにちは"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![
            TranslationEntry {
                locale: "en".to_string(),
                keys: en_keys,
            },
            TranslationEntry {
                locale: "zh".to_string(),
                keys: zh_keys,
            },
            TranslationEntry {
                locale: "ja".to_string(),
                keys: ja_keys,
            },
        ],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalLocales"], 3);
    assert_eq!(json_result["summary"]["successfulLocales"], 3);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 3);

    assert_eq!(fixture.read_locale_file("en").unwrap()["greeting"], "Hello");
    assert_eq!(fixture.read_locale_file("zh").unwrap()["greeting"], "你好");
    assert_eq!(
        fixture.read_locale_file("ja").unwrap()["greeting"],
        "こんにちは"
    );
}

#[tokio::test]
async fn test_add_translations_nested_keys() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("common.submit".to_string(), json!("Submit"));
    keys.insert("common.cancel".to_string(), json!("Cancel"));
    keys.insert("errors.validation.required".to_string(), json!("Required"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "en".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 3);

    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(
        locale_data,
        json!({
            "common": {
                "submit": "Submit",
                "cancel": "Cancel"
            },
            "errors": {
                "validation": {
                    "required": "Required"
                }
            }
        })
    );
}

#[tokio::test]
async fn test_add_translations_array_values() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("items".to_string(), json!(["one", "two", "three"]));
    keys.insert("options".to_string(), json!(["red", "green", "blue"]));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "en".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);

    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(locale_data["items"], json!(["one", "two", "three"]));
    assert_eq!(locale_data["options"], json!(["red", "green", "blue"]));
}

#[tokio::test]
async fn test_add_translations_update_existing_key() {
    let fixture = McpTestFixture::with_messages(vec![(
        "en",
        json!({"greeting": "Old Hello", "other": "value"}),
    )])
    .unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("greeting".to_string(), json!("New Hello"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "en".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 0);
    assert_eq!(json_result["summary"]["totalKeysUpdated"], 1);

    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(locale_data["greeting"], "New Hello");
    assert_eq!(locale_data["other"], "value"); // Unchanged
}

#[tokio::test]
async fn test_add_translations_preserves_existing_keys() {
    let fixture = McpTestFixture::with_messages(vec![(
        "en",
        json!({"existing1": "value1", "existing2": "value2"}),
    )])
    .unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("new_key".to_string(), json!("new value"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "en".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 1);

    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(locale_data["existing1"], "value1");
    assert_eq!(locale_data["existing2"], "value2");
    assert_eq!(locale_data["new_key"], "new value");
}

#[tokio::test]
async fn test_add_translations_creates_new_locale_file() {
    let fixture = McpTestFixture::with_messages(vec![("en", json!({"key": "value"}))]).unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("key".to_string(), json!("新值"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "zh".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 1);

    // Verify new locale file was created
    let locale_data = fixture.read_locale_file("zh").unwrap();
    assert_eq!(locale_data["key"], "新值");
}

#[tokio::test]
async fn test_add_translations_empty_translations_error() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![],
    });

    let result = server.add_translations(params).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("translations array cannot be empty"));
}

#[tokio::test]
async fn test_add_translations_invalid_locale_path_traversal() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("key".to_string(), json!("value"));

    // Try to use path traversal in locale name
    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "../../../etc/passwd".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    // Should fail validation
    assert_eq!(json_result["success"], false);
    assert_eq!(json_result["summary"]["failedLocales"], 1);
    assert_eq!(json_result["summary"]["successfulLocales"], 0);
}

#[tokio::test]
async fn test_add_translations_invalid_locale_absolute_path() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut keys = Map::new();
    keys.insert("key".to_string(), json!("value"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![TranslationEntry {
            locale: "/etc/passwd".to_string(),
            keys,
        }],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], false);
    assert_eq!(json_result["summary"]["failedLocales"], 1);
}

#[tokio::test]
async fn test_add_translations_partial_success() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let mut valid_keys = Map::new();
    valid_keys.insert("key".to_string(), json!("value"));

    let mut invalid_keys = Map::new();
    invalid_keys.insert("key".to_string(), json!("value"));

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![
            TranslationEntry {
                locale: "en".to_string(),
                keys: valid_keys,
            },
            TranslationEntry {
                locale: "../invalid".to_string(),
                keys: invalid_keys,
            },
        ],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], false); // Not all succeeded
    assert_eq!(json_result["summary"]["totalLocales"], 2);
    assert_eq!(json_result["summary"]["successfulLocales"], 1);
    assert_eq!(json_result["summary"]["failedLocales"], 1);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 1);

    // Valid locale should have been created
    let locale_data = fixture.read_locale_file("en").unwrap();
    assert_eq!(locale_data["key"], "value");
}

#[tokio::test]
async fn test_add_translations_summary_statistics() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"existing": "old"})),
        ("zh", json!({"existing": "旧"})),
    ])
    .unwrap();
    let server = GlotMcpServer::new();

    let mut en_keys = Map::new();
    en_keys.insert("existing".to_string(), json!("new")); // Update
    en_keys.insert("new1".to_string(), json!("value1")); // Add

    let mut zh_keys = Map::new();
    zh_keys.insert("new2".to_string(), json!("值2")); // Add
    zh_keys.insert("new3".to_string(), json!("值3")); // Add

    let params = Parameters(AddTranslationsParams {
        project_root_path: fixture.root(),
        translations: vec![
            TranslationEntry {
                locale: "en".to_string(),
                keys: en_keys,
            },
            TranslationEntry {
                locale: "zh".to_string(),
                keys: zh_keys,
            },
        ],
    });

    let result = server.add_translations(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["success"], true);
    assert_eq!(json_result["summary"]["totalLocales"], 2);
    assert_eq!(json_result["summary"]["successfulLocales"], 2);
    assert_eq!(json_result["summary"]["failedLocales"], 0);
    assert_eq!(json_result["summary"]["totalKeysAdded"], 3);
    assert_eq!(json_result["summary"]["totalKeysUpdated"], 1);
}
