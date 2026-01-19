use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
    args::{CheckArgs, CommonArgs},
    commands::runner::{CheckRunner, CheckType},
    config::load_config,
    issue::{Issue, IssueReport, Rule},
    parsers::json::scan_message_files,
};

use super::helpers::process_locale_translation;
use super::types::{
    AddTranslationsParams, AddTranslationsResult, AddTranslationsSummary, ConfigDto, ConfigValues,
    GetConfigParams, GetLocalesParams, HardcodedItem, HardcodedScanResult, HardcodedStats,
    LocaleInfo, LocalesResult, Pagination, PrimaryMissingItem, PrimaryMissingScanResult,
    PrimaryMissingStats, ReplicaLagItem, ReplicaLagScanResult, ReplicaLagStats,
    ScanHardcodedParams, ScanOverviewParams, ScanOverviewResult, ScanPrimaryMissingParams,
    ScanReplicaLagParams, ScanUntranslatedParams, UntranslatedItem, UntranslatedScanResult,
    UntranslatedStats,
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

        let check_args = CheckArgs {
            common: CommonArgs {
                path: std::path::PathBuf::from(path),
                verbose: false,
            },
        };

        let runner = CheckRunner::new(check_args)
            .map_err(|e| McpError::internal_error(format!("Failed to initialize: {}", e), None))?
            .add(CheckType::Hardcoded);

        let result = runner
            .run()
            .map_err(|e| McpError::internal_error(format!("Scan failed: {}", e), None))?;

        // Filter hardcoded issues and collect file stats
        let mut hardcoded_files: HashSet<String> = HashSet::new();
        let all_items: Vec<HardcodedItem> = result
            .issues
            .into_iter()
            .filter_map(|issue| {
                if let Issue::Hardcoded(hardcoded) = issue {
                    hardcoded_files.insert(hardcoded.location.file_path.clone());
                    Some(hardcoded.to_mcp_item())
                } else {
                    None
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

        let check_args = CheckArgs {
            common: CommonArgs {
                path: std::path::PathBuf::from(path),
                verbose: false,
            },
        };

        // Run all checks
        let runner = CheckRunner::new(check_args)
            .map_err(|e| McpError::internal_error(format!("Failed to initialize: {}", e), None))?
            .add(CheckType::Hardcoded)
            .add(CheckType::Missing)
            .add(CheckType::Untranslated);

        let result = runner
            .run()
            .map_err(|e| McpError::internal_error(format!("Scan failed: {}", e), None))?;

        // Count hardcoded issues
        let mut hardcoded_files: HashSet<String> = HashSet::new();
        let hardcoded_count = result
            .issues
            .iter()
            .filter(|i| i.rule() == Rule::HardcodedText)
            .filter_map(|i| i.file_path())
            .map(|fp| {
                hardcoded_files.insert(fp.to_string());
            })
            .count();

        // Count primary missing
        let primary_missing_count = result
            .issues
            .iter()
            .filter(|i| i.rule() == Rule::MissingKey)
            .count();

        // Count replica lag and collect affected locales
        let mut replica_lag_locales: HashSet<String> = HashSet::new();
        let replica_lag_count = result
            .issues
            .iter()
            .filter_map(|i| {
                if let Issue::ReplicaLag(lag) = i {
                    Some(lag)
                } else {
                    None
                }
            })
            .map(|lag| {
                for locale in &lag.missing_in {
                    replica_lag_locales.insert(locale.clone());
                }
            })
            .count();

        let mut replica_lag_locales_vec: Vec<String> = replica_lag_locales.into_iter().collect();
        replica_lag_locales_vec.sort();

        // Count untranslated and collect affected locales
        let mut untranslated_locales: HashSet<String> = HashSet::new();
        let untranslated_count = result
            .issues
            .iter()
            .filter_map(|i| {
                if let Issue::Untranslated(untranslated) = i {
                    Some(untranslated)
                } else {
                    None
                }
            })
            .map(|untranslated| {
                for locale in &untranslated.identical_in {
                    untranslated_locales.insert(locale.clone());
                }
            })
            .count();

        let mut untranslated_locales_vec: Vec<String> = untranslated_locales.into_iter().collect();
        untranslated_locales_vec.sort();

        let overview = ScanOverviewResult {
            hardcoded: HardcodedStats {
                total_count: hardcoded_count,
                file_count: hardcoded_files.len(),
            },
            primary_missing: PrimaryMissingStats {
                total_count: primary_missing_count,
            },
            replica_lag: ReplicaLagStats {
                total_count: replica_lag_count,
                affected_locales: replica_lag_locales_vec,
            },
            untranslated: UntranslatedStats {
                total_count: untranslated_count,
                affected_locales: untranslated_locales_vec,
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

        let check_args = CheckArgs {
            common: CommonArgs {
                path: std::path::PathBuf::from(path),
                verbose: false,
            },
        };

        let runner = CheckRunner::new(check_args)
            .map_err(|e| McpError::internal_error(format!("Failed to initialize: {}", e), None))?
            .add(CheckType::Missing);

        let result = runner
            .run()
            .map_err(|e| McpError::internal_error(format!("Scan failed: {}", e), None))?;

        // Collect primary missing keys
        let all_items: Vec<PrimaryMissingItem> = result
            .issues
            .iter()
            .filter_map(|i| {
                if let Issue::MissingKey(missing) = i {
                    Some(PrimaryMissingItem {
                        key: missing.key.clone(),
                        file_path: missing.location.file_path.clone(),
                        line: missing.location.line,
                    })
                } else {
                    None
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

        let check_args = CheckArgs {
            common: CommonArgs {
                path: std::path::PathBuf::from(path),
                verbose: false,
            },
        };

        let runner = CheckRunner::new(check_args)
            .map_err(|e| McpError::internal_error(format!("Failed to initialize: {}", e), None))?
            .add(CheckType::Missing);

        let result = runner
            .run()
            .map_err(|e| McpError::internal_error(format!("Scan failed: {}", e), None))?;

        // Collect replica lag items
        let all_items: Vec<ReplicaLagItem> = result
            .issues
            .iter()
            .filter_map(|i| {
                if let Issue::ReplicaLag(lag) = i {
                    Some(lag.to_mcp_item())
                } else {
                    None
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

        let check_args = CheckArgs {
            common: CommonArgs {
                path: std::path::PathBuf::from(path),
                verbose: false,
            },
        };

        let runner = CheckRunner::new(check_args)
            .map_err(|e| McpError::internal_error(format!("Failed to initialize: {}", e), None))?
            .add(CheckType::Untranslated);

        let result = runner
            .run()
            .map_err(|e| McpError::internal_error(format!("Scan failed: {}", e), None))?;

        // Collect untranslated items
        let all_items: Vec<UntranslatedItem> = result
            .issues
            .iter()
            .filter_map(|i| {
                if let Issue::Untranslated(untranslated) = i {
                    Some(untranslated.to_mcp_item())
                } else {
                    None
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

        let messages_dir = Path::new(path).join(&config.config.messages_dir);

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

        let messages_dir = Path::new(path).join(&config.config.messages_dir);
        let messages_dir_str = messages_dir.to_string_lossy().to_string();

        let mut locales = Vec::new();

        if messages_dir.exists() && messages_dir.is_dir() {
            let entries = fs::read_dir(&messages_dir).map_err(|e| {
                McpError::internal_error(format!("Failed to read messages directory: {}", e), None)
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    McpError::internal_error(format!("Failed to read directory entry: {}", e), None)
                })?;
                let file_path = entry.path();

                if file_path.extension().and_then(|e| e.to_str()) == Some("json")
                    && let Some(locale) = file_path.file_stem().and_then(|s| s.to_str())
                {
                    // Count keys in the file
                    let key_count = match scan_message_files(&messages_dir) {
                        Ok(scan_result) => scan_result
                            .messages
                            .get(locale)
                            .map(|m| m.len())
                            .unwrap_or(0),
                        Err(_) => 0,
                    };

                    locales.push(LocaleInfo {
                        locale: locale.to_string(),
                        file_path: file_path.to_string_lossy().to_string(),
                        key_count,
                    });
                }
            }
        }

        // Sort locales alphabetically
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

#[tool_handler]
impl ServerHandler for GlotMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Glot MCP helps AI agents complete i18n translation work for next-intl projects.\n\n\
                 Available tools:\n\
                 1. get_config - Get project configuration\n\
                 2. get_locales - Get available locale files and their key counts\n\
                 3. scan_overview - Get statistics of all i18n issues (hardcoded, primary missing, replica lag, untranslated)\n\
                 4. scan_hardcoded - Get detailed hardcoded text list (paginated)\n\
                 5. scan_primary_missing - Get keys missing from primary locale (paginated)\n\
                 6. scan_replica_lag - Get keys missing from non-primary locales (paginated)\n\
                 7. scan_untranslated - Get values identical to primary locale (paginated)\n\
                 8. add_translations - Add keys to locale files\n\n\
                 Recommended Workflow:\n\
                 1. Use scan_overview to understand the overall state\n\
                 2. Fix hardcoded issues first (replace text with t() calls, add keys to primary locale)\n\
                 3. Then fix primary_missing issues (add missing keys to primary locale)\n\
                 4. Fix replica_lag issues (sync keys to other locales)\n\
                 5. Finally fix untranslated issues (translate values that are identical to primary locale)\n\n\
                 IMPORTANT: Follow this order! Fixing hardcoded may create new primary_missing,\n\
                 and fixing primary_missing may create new replica_lag. This order minimizes rework."
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
