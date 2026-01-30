use glot::mcp::{GlotMcpServer, types::ScanTypeMismatchParams};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

use crate::{
    McpTestFixture, assert_pagination, extract_tool_result_json, fixture_with_type_mismatch,
};

// ============================================================================
// scan_type_mismatch tests
// ============================================================================

#[tokio::test]
async fn test_scan_type_mismatch_string_vs_array() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"items": ["one", "two"]})),
        ("zh", json!({"items": "wrong type"})), // Should be array
    ])
    .unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["totalCount"], 1);

    let items = json_result["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);

    let item = &items[0];
    assert_eq!(item["key"], "items");
    assert_eq!(item["expectedType"], "array");
    assert_eq!(item["primaryLocale"], "en");

    let mismatched = item["mismatchedIn"].as_array().unwrap();
    assert_eq!(mismatched.len(), 1);
    assert_eq!(mismatched[0]["locale"], "zh");
    assert_eq!(mismatched[0]["actualType"], "string");
}

#[tokio::test]
async fn test_scan_type_mismatch_array_vs_string() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"message": "Hello"})),
        ("zh", json!({"message": ["错误", "类型"]})), // Should be string
    ])
    .unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["totalCount"], 1);

    let item = &json_result["items"][0];
    assert_eq!(item["key"], "message");
    assert_eq!(item["expectedType"], "string");
    assert_eq!(item["mismatchedIn"][0]["actualType"], "array");
}

#[tokio::test]
async fn test_scan_type_mismatch_nested_keys() {
    // Test that nested leaf values are checked
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"nested": {"items": ["one", "two"]}})),
        ("zh", json!({"nested": {"items": "wrong"}})), // Leaf value mismatch
    ])
    .unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["totalCount"], 1);

    let item = &json_result["items"][0];
    assert_eq!(item["key"], "nested.items");
    assert_eq!(item["expectedType"], "array");
    assert_eq!(item["mismatchedIn"][0]["actualType"], "string");
}

#[tokio::test]
async fn test_scan_type_mismatch_multiple_locales() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"data": ["a", "b"]})),
        ("zh", json!({"data": "wrong"})),
        ("ja", json!({"data": "間違い"})),
        ("fr", json!({"data": ["correct", "type"]})), // This one is correct
    ])
    .unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["totalCount"], 1);

    let item = &json_result["items"][0];
    let mismatched = item["mismatchedIn"].as_array().unwrap();

    // Should detect zh and ja, but not fr
    assert_eq!(mismatched.len(), 2);

    let locales: Vec<&str> = mismatched
        .iter()
        .map(|m| m["locale"].as_str().unwrap())
        .collect();

    assert!(locales.contains(&"zh"));
    assert!(locales.contains(&"ja"));
    assert!(!locales.contains(&"fr"));
}

#[tokio::test]
async fn test_scan_type_mismatch_includes_usage_locations() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"status": ["active", "inactive"]})),
        ("zh", json!({"status": "wrong"})),
    ])
    .unwrap();

    // Create a TSX file that uses this key
    fixture
        .write_tsx_file(
            "app.tsx",
            r#"
        export default function App() {
            const {t} = useTranslations();
            return <div>{t('status')}</div>;
        }
    "#,
        )
        .unwrap();

    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    let item = &json_result["items"][0];

    // Check that usages are included
    assert!(item["usages"].is_array());
    let usages = item["usages"].as_array().unwrap();

    if !usages.is_empty() {
        assert!(usages[0]["filePath"].as_str().unwrap().ends_with("app.tsx"));
        assert!(usages[0]["line"].is_number());
    }

    // totalUsages should be present
    assert!(item["totalUsages"].is_number());
}

#[tokio::test]
async fn test_scan_type_mismatch_pagination() {
    // Create multiple type mismatches
    let fixture = McpTestFixture::with_messages(vec![
        (
            "en",
            json!({
                "key1": ["a"],
                "key2": ["b"],
                "key3": ["c"],
                "key4": ["d"],
                "key5": ["e"]
            }),
        ),
        (
            "zh",
            json!({
                "key1": "wrong",
                "key2": "wrong",
                "key3": "wrong",
                "key4": "wrong",
                "key5": "wrong"
            }),
        ),
    ])
    .unwrap();

    let server = GlotMcpServer::new();

    // Get first page
    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: Some(2),
        offset: Some(0),
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["totalCount"], 5);
    assert_eq!(json_result["items"].as_array().unwrap().len(), 2);
    assert_pagination(&json_result, 0, 2, true);

    // Get second page
    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: Some(2),
        offset: Some(2),
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["items"].as_array().unwrap().len(), 2);
    assert_pagination(&json_result, 2, 2, true);

    // Get last page
    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: Some(2),
        offset: Some(4),
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["items"].as_array().unwrap().len(), 1);
    assert_pagination(&json_result, 4, 2, false);
}

#[tokio::test]
async fn test_scan_type_mismatch_no_issues() {
    let fixture = McpTestFixture::with_messages(vec![
        ("en", json!({"message": "Hello", "items": ["a", "b"]})),
        ("zh", json!({"message": "你好", "items": ["甲", "乙"]})),
    ])
    .unwrap();

    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    assert_eq!(json_result["totalCount"], 0);
    assert_eq!(json_result["items"].as_array().unwrap().len(), 0);
    assert_pagination(&json_result, 0, 50, false);
}

#[tokio::test]
async fn test_scan_type_mismatch_includes_file_locations() {
    let fixture = fixture_with_type_mismatch().unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanTypeMismatchParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_type_mismatch(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    let item = &json_result["items"][0];

    // Check primary file location
    assert!(
        item["primaryFilePath"]
            .as_str()
            .unwrap()
            .ends_with("en.json")
    );
    assert!(item["primaryLine"].is_number());

    // Check mismatched locale file location
    let mismatched = &item["mismatchedIn"][0];
    assert!(
        mismatched["filePath"]
            .as_str()
            .unwrap()
            .ends_with("zh.json")
    );
    assert!(mismatched["line"].is_number());
}

// ============================================================================
// scan_overview tests
// ============================================================================

#[tokio::test]
async fn test_scan_overview_clean_project() {
    use glot::mcp::types::ScanOverviewParams;

    // Create project with clean locales (no issues)
    let fixture = McpTestFixture::with_messages(vec![("en", json!({"key": "value"}))]).unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanOverviewParams {
        project_root_path: fixture.root(),
    });

    let result = server.scan_overview(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    // Clean project should have zero issue counts
    assert_eq!(json_result["hardcoded"]["totalCount"], 0);
    assert_eq!(json_result["typeMismatch"]["totalCount"], 0);
    assert_eq!(json_result["primaryMissing"]["totalCount"], 0);
    assert_eq!(json_result["replicaLag"]["totalCount"], 0);
    assert_eq!(json_result["untranslated"]["totalCount"], 0);
}

#[tokio::test]
async fn test_scan_overview_with_type_mismatches() {
    use glot::mcp::types::ScanOverviewParams;

    // Create a project with type mismatches
    let fixture = fixture_with_type_mismatch().unwrap();
    let server = GlotMcpServer::new();

    let params = Parameters(ScanOverviewParams {
        project_root_path: fixture.root(),
    });

    let result = server.scan_overview(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    // Should detect the type mismatches
    let type_mismatch_count = json_result["typeMismatch"]["totalCount"]
        .as_u64()
        .unwrap_or(0);

    assert!(type_mismatch_count > 0, "Should detect type mismatches");
}

// ============================================================================
// scan_hardcoded tests
// ============================================================================

#[tokio::test]
async fn test_scan_hardcoded_no_issues() {
    use glot::mcp::types::ScanHardcodedParams;

    let fixture = McpTestFixture::with_messages(vec![("en", json!({"key": "value"}))]).unwrap();

    let server = GlotMcpServer::new();

    let params = Parameters(ScanHardcodedParams {
        project_root_path: fixture.root(),
        limit: None,
        offset: None,
    });

    let result = server.scan_hardcoded(params).await.unwrap();
    let json_result = extract_tool_result_json(&result);

    // Should work even with no TSX files
    assert_eq!(json_result["totalCount"], 0);
    assert_eq!(json_result["totalFileCount"], 0);
}
