# commands/mod.rs Refactoring Plan

## Goal
Reduce `src-tauri/src/commands/mod.rs` from 8,869 lines to ≤2,000 lines per metrics.md §1.4 readability target.

## Constraint
- All submodules currently use `use super::*;` so they depend on types/functions being accessible from mod.rs
- The `timed_sync!` and `timed_async!` macros must remain in mod.rs (they're used via `super::*` in submodules)
- `lib.rs` imports specific command names from `crate::commands::` — all pub command functions must remain accessible via re-exports
- Do NOT change any public API or Tauri command signatures
- Every extraction must compile and pass `cargo check`

## Extraction Plan (by new/existing target module)

### 1. NEW: `types.rs` (~500 lines)
Move ALL struct/enum definitions that are shared types (not specific to one submodule):
- SystemStatus, OpenclawUpdateCheck, ModelCatalogProviderCache, OpenclawCommandOutput (+ impl From), RescueBotCommandResult, RescueBotManageResult, RescuePrimaryCheckItem, RescuePrimaryIssue, RescuePrimaryDiagnosisResult, RescuePrimarySummary, RescuePrimarySectionResult, RescuePrimarySectionItem, RescuePrimaryRepairStep, RescuePrimaryPendingAction, RescuePrimaryRepairResult, ExtractModelProfilesResult, ExtractModelProfileEntry, OpenclawUpdateCache, ModelSummary, ChannelSummary, MemoryFileSummary, MemorySummary, AgentSessionSummary, SessionFile, SessionAnalysis, AgentSessionAnalysis, SessionSummary, ModelCatalogModel, ModelCatalogProvider, ChannelNode, DiscordGuildChannel, ProviderAuthSuggestion, ModelBinding, HistoryItem, HistoryPage, FixResult, AgentOverview, StatusLight, StatusExtra, SshBottleneck, SshConnectionStage, SshConnectionProfile, ResolvedApiKey, ResolvedCredentialKind, BackupInfo, RescueBotAction (+ impl), InternalAuthKind, ResolvedCredentialSource, InternalProviderCredential, SecretRef, ChannelNameCacheEntry, InventorySummary
- Also the type alias: `pub type ModelProfile = clawpal_core::profile::ModelProfile;`

### 2. NEW: `cli.rs` (~200 lines)
Move CLI runner functions:
- run_openclaw_raw, run_openclaw_raw_timeout, run_openclaw_dynamic
- OPENCLAW_VERSION_CACHE static, clear_openclaw_version_cache, resolve_openclaw_version
- shell_escape, expand_tilde
- extract_last_json_array
- parse_json_from_openclaw_output

### 3. NEW: `version.rs` (~250 lines)
Move version/update checking:
- extract_version_from_text, compare_semver, normalize_semver_components
- normalize_openclaw_release_tag, query_openclaw_latest_github_release
- unix_timestamp_secs, format_timestamp_from_unix
- openclaw_update_cache_path, read_openclaw_update_cache, save_openclaw_update_cache
- check_openclaw_update_cached, resolve_openclaw_latest_release_cached
- Tests: openclaw_update_tests

### 4. NEW: `credentials.rs` (~900 lines)
Move credential resolution:
- resolve_profile_credential_with_priority, resolve_profile_api_key_with_priority, resolve_profile_api_key
- collect_provider_credentials_for_internal, collect_provider_credentials_from_paths, collect_provider_credentials_from_profiles
- augment_provider_credentials_from_openclaw_config, resolve_provider_credential_from_config_entry
- resolve_credential_from_agent_auth_profiles, resolve_credential_from_local_auth_store_dir
- local_openclaw_roots, auth_ref_lookup_keys
- resolve_key_from_auth_store_json, resolve_key_from_auth_store_json_with_env
- resolve_credential_from_auth_store_json, resolve_credential_from_auth_store_json_with_env
- SecretRef functions: try_parse_secret_ref, normalize_secret_provider_name, load_secret_provider_config, secret_ref_allowed_in_provider_cfg, expand_home_path, resolve_secret_ref_file_with_provider_config, read_trusted_dirs, resolve_secret_ref_exec_with_provider_config, resolve_secret_ref_with_provider_config, resolve_secret_ref_with_env, resolve_secret_ref_file, local_env_lookup
- collect_secret_ref_env_names_from_entry, collect_secret_ref_env_names_from_auth_store
- extract_credential_from_auth_entry, extract_credential_from_auth_entry_with_env
- mask_api_key, is_valid_env_var_name
- infer_auth_kind, provider_env_var_candidates, is_oauth_provider_alias, is_oauth_auth_ref, infer_resolved_credential_kind
- provider_supports_optional_api_key, default_base_url_for_provider
- run_provider_probe, truncate_error_text, MAX_ERROR_SNIPPET_CHARS
- Tests: secret_ref_tests

### 5. NEW: `channels.rs` (~400 lines)
Move channel functions:
- collect_channel_nodes, walk_channel_nodes, is_channel_like_node, resolve_channel_type, resolve_channel_mode, collect_channel_allowlist
- enrich_channel_display_names, save_json_cache, resolve_channel_node_identity, channel_last_segment, channel_node_local_name, channel_lookup_node
- collect_channel_summary, collect_channel_model_overrides, collect_channel_model_overrides_list, collect_channel_paths
- read_model_value (used widely — may need to stay in mod.rs or types.rs)

### 6. NEW: `discord.rs` (~300 lines)
Move Discord functions:
- DISCORD_REST_USER_AGENT, fetch_discord_guild_name, fetch_discord_guild_channels
- collect_discord_config_guild_ids, collect_discord_config_guild_name_fallbacks
- collect_discord_cache_guild_name_fallbacks, parse_discord_cache_guild_name_fallbacks
- parse_resolve_name_map, parse_directory_group_channel_ids
- Tests: discord_directory_parse_tests

### 7. EXPAND: `rescue.rs` (move ~2000 lines of rescue logic)
Move ALL rescue bot internal functions:
- normalize_profile_name, build_profile_command, build_gateway_status_command
- command_detail, gateway_output_ok, gateway_output_detail
- infer_rescue_bot_runtime_state
- rescue_section_order, rescue_section_title, rescue_section_docs_url
- section_item_status_from_issue, classify_rescue_check_section, classify_rescue_issue_section
- has_unreadable_primary_config_issue, config_item
- build_rescue_primary_sections, build_rescue_primary_summary
- doc_guidance_section_from_url, classify_doc_guidance_section
- build_doc_resolve_request, apply_doc_guidance_to_diagnosis
- collect_local_rescue_runtime_checks, collect_remote_rescue_runtime_checks
- build_rescue_primary_diagnosis
- diagnose_primary_via_rescue_local, diagnose_primary_via_rescue_remote
- collect_repairable_primary_issue_ids
- build_primary_issue_fix_command, build_primary_doctor_fix_command
- should_run_primary_doctor_fix, should_refresh_rescue_helper_permissions
- build_step_detail
- run_local_gateway_restart_with_fallback, run_local_rescue_permission_refresh, run_local_primary_doctor_fix
- run_remote_gateway_restart_with_fallback, run_remote_rescue_permission_refresh, run_remote_primary_doctor_fix
- repair_primary_via_rescue_local, repair_primary_via_rescue_remote
- resolve_local_rescue_profile_state, resolve_remote_rescue_profile_state
- build_rescue_bot_command_plan
- command_failure_message, is_gateway_restart_command, is_gateway_restart_timeout, is_rescue_cleanup_noop
- run_local_rescue_bot_command, is_gateway_status_command_output_incompatible, strip_gateway_status_json_flag
- run_local_primary_doctor_with_fallback, run_local_gateway_restart_fallback
- Tests: rescue_bot_tests

### 8. EXPAND: existing modules
- `sessions.rs`: move analyze_sessions_sync, delete_sessions_by_ids_sync, preview_session_sync, list_session_files_detailed, collect_session_files_in_scope, clear_agent_and_global_sessions, clear_directory_contents, collect_session_overview, collect_file_inventory, collect_file_inventory_with_limit
- `model.rs`: move load_model_catalog, select_catalog_from_cache, parse_model_catalog_from_cli_output, extract_model_catalog_from_cli, cache_model_catalog, model_catalog_cache_path, remote_model_catalog_cache_path, read_model_catalog_cache, save_model_catalog_cache, normalize_model_ref, collect_model_bindings, find_profile_by_model, resolve_auth_ref_for_provider, collect_model_summary, collect_main_auth_model_candidates. Tests: model_catalog_cache_tests, model_value_tests
- `profiles.rs`: move load_model_profiles, save_model_profiles, model_profiles_path, profile_to_model_value, sync_profile_auth_to_main_agent_with_source, maybe_sync_main_auth_for_model_value, maybe_sync_main_auth_for_model_value_with_source, sync_main_auth_for_config, sync_main_auth_for_active_config, resolve_full_api_key. Tests: model_profile_upsert_tests
- `backup.rs`: move copy_dir_recursive, dir_size, restore_dir_recursive
- `config.rs` or `util.rs`: move write_config_with_snapshot, set_nested_value, set_agent_model_value
- `agent.rs`: move agent_entries_from_cli_json, count_agent_entries_from_cli_json, parse_agents_cli_output, agent_has_sessions, collect_agent_ids. Tests: parse_agents_cli_output_tests
- `ssh.rs` (remote ops): move remote_write_config_with_snapshot, remote_resolve_openclaw_config_path, remote_read_openclaw_config_text_and_json, run_remote_rescue_bot_command, run_remote_openclaw_raw, run_remote_openclaw_dynamic, run_remote_primary_doctor_with_fallback, run_remote_gateway_restart_fallback, is_remote_missing_path_error, read_remote_env_var, resolve_remote_key_from_agent_auth_profiles, resolve_remote_openclaw_roots, resolve_remote_profile_base_url, resolve_remote_profile_api_key, RemoteAuthCache + impl
- `cron.rs`: move parse_cron_jobs

## Approach
1. Create new modules one at a time
2. After each extraction, run `cargo check` to verify compilation
3. Each new module uses `use super::*;` or explicit imports from sibling modules
4. Update mod.rs to declare new modules and re-export their public items
5. Proceed incrementally — rescue and credentials are the two biggest blocks

## What stays in mod.rs (~500 lines target)
- Macros (timed_sync!, timed_async!)
- use/import statements
- mod declarations for all submodules
- pub use re-exports
- REMOTE_OPENCLAW_CONFIG_PATH_CACHE static
- A few small utility functions that are genuinely cross-cutting: truncated_json_debug, local_health_instance, local_cli_cache_key
- read_model_value (widely used across many modules)
- collect_memory_overview (small, used by overview)
