use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use serde_json;

use crate::{
    config::load_config,
    core::parsers::json::scan_message_files,
    core::{CheckContext, ResolvedKeyUsage},
    issues::{
        HardcodedTextIssue, MissingKeyIssue, ReplicaLagIssue, TypeMismatchIssue, UntranslatedIssue,
    },
    rules::{
        hardcoded::check_hardcoded_text_issues, missing::check_missing_keys_issues,
        replica_lag::check_replica_lag_issues, type_mismatch::check_type_mismatch_issues,
        untranslated::check_untranslated_issues,
    },
};

use crate::mcp::helpers::process_locale_translation;
use crate::mcp::types::{
    AddTranslationsParams, AddTranslationsResult, AddTranslationsSummary, ConfigDto, ConfigValues,
    GetConfigParams, GetLocalesParams, HardcodedItem, HardcodedScanResult, HardcodedStats,
    KeyUsageLocation, LocaleInfo, LocalesResult, Pagination, PrimaryMissingItem,
    PrimaryMissingScanResult, PrimaryMissingStats, ReplicaLagItem, ReplicaLagScanResult,
    ReplicaLagStats, ScanHardcodedParams, ScanOverviewParams, ScanOverviewResult,
    ScanPrimaryMissingParams, ScanReplicaLagParams, ScanTypeMismatchParams, ScanUntranslatedParams,
    TypeMismatchItem, TypeMismatchLocale, TypeMismatchScanResult, TypeMismatchStats,
    UntranslatedItem, UntranslatedScanResult, UntranslatedStats,
};

#[derive(Clone)]
pub struct GlotMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GlotMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Scan for hardcoded text that should use translations
    #[tool(
        description = "Scan for hardcoded text in JSX/TSX files that should use translations. Returns paginated list of issues."
    )]
    async fn scan_hardcoded(
        &self,
        params: Parameters<ScanHardcodedParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;
        let limit = params.0.limit.map(|v| v as usize).unwrap_or(20).min(100);
        let offset = params.0.offset.map(|v| v as usize).unwrap_or(0);

        let ctx = create_context(path)?;
        let issues = check_hardcoded_text_issues(&ctx);

        let mut hardcoded_files: HashSet<String> = HashSet::new();
        let all_items: Vec<HardcodedItem> = issues
            .into_iter()
            .map(|issue| {
                let HardcodedTextIssue { context, text } = issue;
                hardcoded_files.insert(context.file_path().to_string());
                HardcodedItem {
                    file_path: context.file_path().to_string(),
                    line: context.line(),
                    col: context.col(),
                    text,
                    source_line: context.source_line,
                }
            })
            .collect();

        let total_count = all_items.len();
        let total_file_count = hardcoded_files.len();

        // Apply pagination
        let paginated: Vec<HardcodedItem> =
            all_items.into_iter().skip(offset).take(limit).collect();

        let has_more = offset + paginated.len() < total_count;

        let scan_result = HardcodedScanResult {
            total_count,
            total_file_count,
            items: paginated,
            pagination: Pagination {
                offset,
                limit,
                has_more,
            },
        };

        let json_str = serde_json::to_string_pretty(&scan_result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Get overview statistics of all i18n issues
    #[tool(
        description = "Get statistics of all i18n issues without detailed items. Use this first to understand the overall state before diving into details."
    )]
    async fn scan_overview(
        &self,
        params: Parameters<ScanOverviewParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;

        let ctx = create_context(path)?;

        let hardcoded = check_hardcoded_text_issues(&ctx);
        let primary_missing = check_missing_keys_issues(&ctx);
        let replica_lag = check_replica_lag_issues(&ctx);
        let untranslated = check_untranslated_issues(&ctx);
        let type_mismatch = check_type_mismatch_issues(&ctx);

        let mut hardcoded_files: HashSet<String> = HashSet::new();
        for issue in &hardcoded {
            hardcoded_files.insert(issue.context.file_path().to_string());
        }

        let mut replica_lag_locales: HashSet<String> = HashSet::new();
        for issue in &replica_lag {
            for locale in &issue.missing_in {
                replica_lag_locales.insert(locale.clone());
            }
        }

        let mut untranslated_locales: HashSet<String> = HashSet::new();
        for issue in &untranslated {
            for locale in &issue.identical_in {
                untranslated_locales.insert(locale.clone());
            }
        }

        let mut type_mismatch_locales: HashSet<String> = HashSet::new();
        for issue in &type_mismatch {
            for mismatch in &issue.mismatched_in {
                type_mismatch_locales.insert(mismatch.locale.clone());
            }
        }

        let mut replica_lag_locales_vec: Vec<String> = replica_lag_locales.into_iter().collect();
        replica_lag_locales_vec.sort();

        let mut untranslated_locales_vec: Vec<String> = untranslated_locales.into_iter().collect();
        untranslated_locales_vec.sort();

        let mut type_mismatch_locales_vec: Vec<String> =
            type_mismatch_locales.into_iter().collect();
        type_mismatch_locales_vec.sort();

        let overview = ScanOverviewResult {
            hardcoded: HardcodedStats {
                total_count: hardcoded.len(),
                file_count: hardcoded_files.len(),
            },
            primary_missing: PrimaryMissingStats {
                total_count: primary_missing.len(),
            },
            replica_lag: ReplicaLagStats {
                total_count: replica_lag.len(),
                affected_locales: replica_lag_locales_vec,
            },
            untranslated: UntranslatedStats {
                total_count: untranslated.len(),
                affected_locales: untranslated_locales_vec,
            },
            type_mismatch: TypeMismatchStats {
                total_count: type_mismatch.len(),
                affected_locales: type_mismatch_locales_vec,
            },
        };

        let json_str = serde_json::to_string_pretty(&overview).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Scan for keys missing from primary locale
    #[tool(
        description = "Scan for keys used in code but missing from primary locale. Returns paginated list."
    )]
    async fn scan_primary_missing(
        &self,
        params: Parameters<ScanPrimaryMissingParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;
        let limit = params.0.limit.map(|v| v as usize).unwrap_or(50).min(100);
        let offset = params.0.offset.map(|v| v as usize).unwrap_or(0);

        let ctx = create_context(path)?;
        let issues = check_missing_keys_issues(&ctx);

        let all_items: Vec<PrimaryMissingItem> = issues
            .into_iter()
            .map(|issue| {
                let MissingKeyIssue {
                    context,
                    key,
                    from_schema,
                } = issue;
                PrimaryMissingItem {
                    key,
                    file_path: context.file_path().to_string(),
                    line: context.line(),
                    source: from_schema
                        .as_ref()
                        .map(|(name, _)| format!("from schema \"{}\"", name)),
                }
            })
            .collect();

        let total_count = all_items.len();

        // Apply pagination
        let paginated: Vec<PrimaryMissingItem> =
            all_items.into_iter().skip(offset).take(limit).collect();

        let has_more = offset + paginated.len() < total_count;

        let scan_result = PrimaryMissingScanResult {
            total_count,
            items: paginated,
            pagination: Pagination {
                offset,
                limit,
                has_more,
            },
        };

        let json_str = serde_json::to_string_pretty(&scan_result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Scan for keys missing from non-primary locales (replica lag)
    #[tool(
        description = "Scan for keys that exist in primary locale but missing in other locales. Returns paginated list with code usage locations to help prioritize fixes."
    )]
    async fn scan_replica_lag(
        &self,
        params: Parameters<ScanReplicaLagParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;
        let limit = params.0.limit.map(|v| v as usize).unwrap_or(50).min(100);
        let offset = params.0.offset.map(|v| v as usize).unwrap_or(0);

        let ctx = create_context(path)?;
        let issues = check_replica_lag_issues(&ctx);

        let all_items: Vec<ReplicaLagItem> = issues
            .into_iter()
            .map(|issue| {
                let ReplicaLagIssue {
                    context,
                    primary_locale,
                    missing_in,
                    usages,
                } = issue;
                let (usages, total_usages) = to_usage_locations(&usages);
                let file_path = context.file_path().to_string();
                let line = context.line();
                ReplicaLagItem {
                    key: context.key,
                    value: context.value,
                    file_path,
                    line,
                    exists_in: primary_locale,
                    missing_in,
                    usages,
                    total_usages,
                }
            })
            .collect();

        let total_count = all_items.len();

        // Apply pagination
        let paginated: Vec<ReplicaLagItem> =
            all_items.into_iter().skip(offset).take(limit).collect();

        let has_more = offset + paginated.len() < total_count;

        let scan_result = ReplicaLagScanResult {
            total_count,
            items: paginated,
            pagination: Pagination {
                offset,
                limit,
                has_more,
            },
        };

        let json_str = serde_json::to_string_pretty(&scan_result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Scan for values that are identical to primary locale (possibly not translated)
    #[tool(
        description = "Scan for translation values identical to primary locale. These may indicate text was copied without translation. Returns paginated list with code usage locations."
    )]
    async fn scan_untranslated(
        &self,
        params: Parameters<ScanUntranslatedParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;
        let limit = params.0.limit.map(|v| v as usize).unwrap_or(50).min(100);
        let offset = params.0.offset.map(|v| v as usize).unwrap_or(0);

        let ctx = create_context(path)?;
        let issues = check_untranslated_issues(&ctx);

        let all_items: Vec<UntranslatedItem> = issues
            .into_iter()
            .map(|issue| {
                let UntranslatedIssue {
                    context,
                    primary_locale,
                    identical_in,
                    usages,
                } = issue;
                let (usages, total_usages) = to_usage_locations(&usages);
                let file_path = context.file_path().to_string();
                let line = context.line();
                UntranslatedItem {
                    key: context.key,
                    value: context.value,
                    file_path,
                    line,
                    identical_in,
                    primary_locale,
                    usages,
                    total_usages,
                }
            })
            .collect();

        let total_count = all_items.len();

        // Apply pagination
        let paginated: Vec<UntranslatedItem> =
            all_items.into_iter().skip(offset).take(limit).collect();

        let has_more = offset + paginated.len() < total_count;

        let scan_result = UntranslatedScanResult {
            total_count,
            items: paginated,
            pagination: Pagination {
                offset,
                limit,
                has_more,
            },
        };

        let json_str = serde_json::to_string_pretty(&scan_result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Scan for type mismatches between primary and replica locales
    #[tool(
        description = "Scan for type mismatches between locales. For example: primary has array but replica has string. This causes runtime crashes. Returns paginated list with code usage locations."
    )]
    async fn scan_type_mismatch(
        &self,
        params: Parameters<ScanTypeMismatchParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;
        let limit = params.0.limit.map(|v| v as usize).unwrap_or(50).min(100);
        let offset = params.0.offset.map(|v| v as usize).unwrap_or(0);

        let ctx = create_context(path)?;
        let issues = check_type_mismatch_issues(&ctx);

        let all_items: Vec<TypeMismatchItem> = issues
            .into_iter()
            .map(|issue| {
                let TypeMismatchIssue {
                    context,
                    expected_type,
                    primary_locale,
                    mismatched_in,
                    usages,
                } = issue;
                let (usages, total_usages) = to_usage_locations(&usages);
                let mismatched_in = mismatched_in
                    .into_iter()
                    .map(|mismatch| TypeMismatchLocale {
                        locale: mismatch.locale,
                        actual_type: mismatch.actual_type.to_string(),
                        file_path: mismatch.location.file_path,
                        line: mismatch.location.line,
                    })
                    .collect();

                let primary_file_path = context.file_path().to_string();
                let primary_line = context.line();
                TypeMismatchItem {
                    key: context.key,
                    expected_type: expected_type.to_string(),
                    primary_file_path,
                    primary_line,
                    primary_locale,
                    mismatched_in,
                    usages,
                    total_usages,
                }
            })
            .collect();

        let total_count = all_items.len();

        // Apply pagination
        let paginated: Vec<TypeMismatchItem> =
            all_items.into_iter().skip(offset).take(limit).collect();

        let has_more = offset + paginated.len() < total_count;

        let scan_result = TypeMismatchScanResult {
            total_count,
            items: paginated,
            pagination: Pagination {
                offset,
                limit,
                has_more,
            },
        };

        let json_str = serde_json::to_string_pretty(&scan_result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Add translation keys to multiple locale files
    #[tool(
        description = "Add translation keys to multiple locale files. Supports nested keys (e.g., 'common.title') and string arrays."
    )]
    async fn add_translations(
        &self,
        params: Parameters<AddTranslationsParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;
        let translations = &params.0.translations;

        // Validate translations is not empty
        if translations.is_empty() {
            return Err(McpError::invalid_params(
                "translations array cannot be empty",
                None,
            ));
        }

        // Load config to get messages_dir
        let config = load_config(Path::new(path))
            .map_err(|e| McpError::internal_error(format!("Failed to load config: {}", e), None))?;

        let messages_dir = resolve_messages_dir(Path::new(path), &config.config.messages_dir);

        let mut results = Vec::new();
        let mut total_keys_added = 0;
        let mut total_keys_updated = 0;
        let mut successful_locales = 0;
        let mut failed_locales = 0;

        for translation in translations {
            // Convert TranslationEntry to serde_json::Value for process_locale_translation
            let translation_value = serde_json::to_value(translation).map_err(|e| {
                McpError::internal_error(format!("Failed to serialize translation: {}", e), None)
            })?;
            let locale_result = process_locale_translation(&translation_value, &messages_dir);

            match &locale_result {
                Ok(result) => {
                    total_keys_added += result.added_count.unwrap_or(0);
                    total_keys_updated += result.updated_count.unwrap_or(0);
                    successful_locales += 1;
                }
                Err(_) => {
                    failed_locales += 1;
                }
            }

            results.push(locale_result.unwrap_or_else(|e| e.into_result()));
        }

        let all_success = failed_locales == 0;

        let result = AddTranslationsResult {
            success: all_success,
            results,
            summary: AddTranslationsSummary {
                total_locales: translations.len(),
                successful_locales,
                failed_locales,
                total_keys_added,
                total_keys_updated,
            },
        };

        let json_str = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Get available locales and their file paths
    #[tool(description = "Get available locales and their file paths.")]
    async fn get_locales(
        &self,
        params: Parameters<GetLocalesParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = &params.0.project_root_path;

        // Load config to get messages_dir and primary_locale
        let config = load_config(Path::new(path))
            .map_err(|e| McpError::internal_error(format!("Failed to load config: {}", e), None))?;

        let messages_dir = resolve_messages_dir(Path::new(path), &config.config.messages_dir);
        let messages_dir_str = messages_dir.to_string_lossy().to_string();

        let scan_result = scan_message_files(&messages_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to scan messages: {}", e), None)
        })?;

        let mut locales: Vec<LocaleInfo> = scan_result
            .messages
            .values()
            .map(|messages| LocaleInfo {
                locale: messages.locale.clone(),
                file_path: messages.file_path.clone(),
                key_count: messages.len(),
            })
            .collect();

        locales.sort_by(|a, b| a.locale.cmp(&b.locale));

        let result = LocalesResult {
            messages_dir: messages_dir_str,
            primary_locale: config.config.primary_locale,
            locales,
        };

        let json_str = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Get the current glot configuration
    #[tool(description = "Get the current glot configuration.")]
    async fn get_config(
        &self,
        params: Parameters<GetConfigParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = Path::new(&params.0.project_root_path);

        let result = load_config(path)
            .map_err(|e| McpError::internal_error(format!("Failed to load config: {}", e), None))?;

        let config_dto = ConfigDto {
            from_file: result.from_file,
            config: ConfigValues::from(result.config),
        };

        let json_str = serde_json::to_string_pretty(&config_dto).map_err(|e| {
            McpError::internal_error(format!("JSON serialization failed: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }
}

fn create_context(path: &str) -> Result<CheckContext, McpError> {
    CheckContext::new(&PathBuf::from(path), false)
        .map_err(|e| McpError::internal_error(format!("Failed to initialize: {}", e), None))
}

fn to_usage_locations(usages: &[ResolvedKeyUsage]) -> (Vec<KeyUsageLocation>, usize) {
    let total = usages.len();
    let items = usages
        .iter()
        .take(3)
        .map(|usage| KeyUsageLocation {
            file_path: usage.context.file_path().to_string(),
            line: usage.context.line(),
            col: usage.context.col(),
        })
        .collect();
    (items, total)
}

fn resolve_messages_dir(root_dir: &Path, messages_dir: &str) -> PathBuf {
    let p = Path::new(messages_dir);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        let is_cur_dir = root_dir
            .components()
            .all(|c| matches!(c, Component::CurDir));
        if is_cur_dir {
            p.to_path_buf()
        } else {
            let rel = p.strip_prefix(Path::new(".")).unwrap_or(p);
            root_dir.join(rel)
        }
    }
}

#[tool_handler]
impl ServerHandler for GlotMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Glot MCP helps AI agents complete i18n translation work for next-intl projects.\n\n\
                 Available tools:\n\
                 1. get_config - Get project configuration\n\
                 2. get_locales - Get available locale files and their key counts\n\
                 3. scan_overview - Get statistics of all i18n issues (hardcoded, primary missing, replica lag, untranslated, type mismatch)\n\
                 4. scan_hardcoded - Get detailed hardcoded text list (paginated)\n\
                 5. scan_primary_missing - Get keys missing from primary locale (paginated)\n\
                 6. scan_replica_lag - Get keys missing from non-primary locales (paginated)\n\
                 7. scan_untranslated - Get values identical to primary locale (paginated)\n\
                 8. scan_type_mismatch - Get type mismatches between locales (paginated)\n\
                 9. add_translations - Add keys to locale files\n\n\
                 Recommended Workflow:\n\
                 1. Use scan_overview to understand the overall state\n\
                 2. Fix type_mismatch issues FIRST (these cause runtime crashes!)\n\
                 3. Fix hardcoded issues (replace text with t() calls, add keys to primary locale)\n\
                 4. Then fix primary_missing issues (add missing keys to primary locale)\n\
                 5. Fix replica_lag issues (sync keys to other locales)\n\
                 6. Finally fix untranslated issues (translate values that are identical to primary locale)\n\n\
                 IMPORTANT: Follow this order! Type mismatches are critical errors that cause crashes.\n\
                 Fixing hardcoded may create new primary_missing, and fixing primary_missing may create new replica_lag."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Entry point for MCP server
pub fn run_server() -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let service = GlotMcpServer::new();
            let server = service.serve(rmcp::transport::stdio()).await?;
            server.waiting().await?;
            Ok(())
        })
}
