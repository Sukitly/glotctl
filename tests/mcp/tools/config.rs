use glot::mcp::{
    GlotMcpServer,
    types::{GetConfigParams, GetLocalesParams},
};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use crate::{McpTestFixture, extract_tool_result_json};

// ============================================================================
// get_config tests
// ============================================================================

#[tokio::test]
async fn test_get_config_defaults() {
    let fixture = McpTestFixture::with_messages(vec![("en", json!({}))]).unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(GetConfigParams {
        project_root_path: fixture.root(),
    });

    let result = server.get_config(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    // Check default config values
    assert_eq!(json_result["config"]["messagesRoot"], "./messages");
    assert_eq!(json_result["config"]["primaryLocale"], "en");
    assert!(json_result["config"]["includes"].is_array());
    assert_eq!(json_result["fromFile"], false);
}

#[tokio::test]
async fn test_get_config_from_glotrc() {
    let fixture = McpTestFixture::new().unwrap();

    // Write custom config
    fixture
        .write_config(&json!({
            "messagesRoot": "locales",
            "primaryLocale": "zh",
            "sourcePatterns": ["**/*.tsx"]
        }))
        .unwrap();

    let server = GlotMcpServer::new();

    let params = Parameters(GetConfigParams {
        project_root_path: fixture.root(),
    });

    let result = server.get_config(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    // Should use custom config
    assert_eq!(json_result["config"]["messagesRoot"], "locales");
    assert_eq!(json_result["config"]["primaryLocale"], "zh");
}

// ============================================================================
// get_locales tests
// ============================================================================

#[tokio::test]
async fn test_get_locales_single() {
    let fixture = McpTestFixture::with_messages(vec![("en", json!({"key": "value"}))]).unwrap();

    let server = GlotMcpServer::new();

    let params = Parameters(GetLocalesParams {
        project_root_path: fixture.root(),
    });

    let result = server.get_locales(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    let locales = json_result["locales"].as_array().unwrap();
    assert_eq!(locales.len(), 1);
    assert_eq!(locales[0]["locale"], "en");
    assert!(
        locales[0]["filePath"]
            .as_str()
            .unwrap()
            .ends_with("en.json")
    );
    assert_eq!(locales[0]["keyCount"], 1);
}

#[tokio::test]
async fn test_get_locales_multiple() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"a": "1", "b": "2"})),
        ("zh", json!({"a": "一", "b": "二", "c": "三"})),
        ("ja", json!({"a": "壱"})),
    ])
    .unwrap();

    let server = GlotMcpServer::new();

    let params = Parameters(GetLocalesParams {
        project_root_path: fixture.root(),
    });

    let result = server.get_locales(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    let locales = json_result["locales"].as_array().unwrap();
    assert_eq!(locales.len(), 3);

    // Check key counts
    let en = locales.iter().find(|l| l["locale"] == "en").unwrap();
    assert_eq!(en["keyCount"], 2);

    let zh = locales.iter().find(|l| l["locale"] == "zh").unwrap();
    assert_eq!(zh["keyCount"], 3);

    let ja = locales.iter().find(|l| l["locale"] == "ja").unwrap();
    assert_eq!(ja["keyCount"], 1);
}

#[tokio::test]
async fn test_get_locales_empty_messages_dir() {
    let fixture = McpTestFixture::new().unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(GetLocalesParams {
        project_root_path: fixture.root(),
    });

    let result = server.get_locales(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    let locales = json_result["locales"].as_array().unwrap();
    assert_eq!(locales.len(), 0);
}
