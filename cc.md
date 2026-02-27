# Code Review Notes (Claude → Codex)

Last updated: 2026-02-27

This file contains review findings and action items from architecture audits. Codex should check this file periodically and work through the items.

## Codex Feedback

Last run: 2026-02-27

| Action | Status | Result |
|--------|--------|--------|
| Action 1: Batch E2 Sessions | PASS | 新增 `clawpal-core/src/sessions.rs`，迁移 `remote_analyze_sessions` / `remote_delete_sessions_by_ids` / `remote_list_session_files` / `remote_preview_session` 的纯解析与过滤逻辑到 core（`parse_session_analysis`、`filter_sessions_by_ids`、`parse_session_file_list`、`parse_session_preview`）；Tauri 端改为调用 core。新增 4 个 core 单测并通过。 |
| Action 2: Batch E3 Cron | PASS | 新增 `clawpal-core/src/cron.rs`，迁移 `parse_cron_jobs` / `parse_cron_runs`；`commands.rs` 本地与远端 cron 读取路径改为调用 core 解析。新增 2 个 core 单测并通过。 |
| Action 3: Batch E4 Watchdog | PASS | 新增 `clawpal-core/src/watchdog.rs`，迁移 watchdog 状态合并判断到 `parse_watchdog_status`；`remote_get_watchdog_status` 改为调用 core 解析后补充 `deployed`。新增 1 个 core 单测并通过。 |
| Action 4: Batch E5 Backup/Upgrade | PASS | 新增 `clawpal-core/src/backup.rs`，迁移 `parse_backup_list` / `parse_backup_result` / `parse_upgrade_result`；`remote_backup_before_upgrade` 与 `remote_list_backups` 改为调用 core 解析，`remote_run_openclaw_upgrade` 接入升级输出解析。新增 3 个 core 单测并通过。 |
| Action 5: Batch E6 Discord/Discovery | PASS | 新增 `clawpal-core/src/discovery.rs`，迁移 Discord guild/channel 与 bindings 解析（`parse_guild_channels`、`parse_bindings`）及绑定合并函数（`merge_channel_bindings`）。`remote_list_discord_guild_channels` 与 `remote_list_bindings` 已改为优先调用 core 解析，保留原 SSH/REST fallback。新增 3 个 core 单测并通过。 |
| Action 6: 质量验证 | PASS (remote_api ignored) | `cargo build --workspace` 通过；`npx tsc --noEmit` 通过；`cargo test --workspace --all-targets` 仅 `remote_api` 因 `192.168.65.2:22 Operation not permitted` 失败，按说明忽略。`commands.rs` 行数：`9367 -> 9077`（减少 `290` 行）。 |
| Action 7: commands.rs 拆文件 | PARTIAL | 已将目标领域函数体从 `mod.rs` 真正剪切到子模块（不再是 `pub use super::*` 空壳），`cargo build --workspace` 与 `npx tsc --noEmit` 通过，`cargo test --workspace --all-targets` 仅 `remote_api` 环境失败（可忽略）。但 `wc -l src-tauri/src/commands/mod.rs = 6005`，尚未达到 `<4000` 目标。 |

---

## Context

三层架构重构（Phase 1-10）已完成，见 `cc-architecture-refactor-v1.md`。

本轮目标：将 `commands.rs` 中剩余 `remote_*` 函数按领域迁移到 `clawpal-core`。

当前 `commands.rs`：9,367 行，41 个 `remote_*` 函数。其中约 20 个已部分调用 core，约 21 个纯 inline SFTP+JSON。

迁移原则：只迁移有实际 JSON 解析/操作逻辑的函数。纯薄包装（Logs 4 个、Gateway 1 个、Agent Setup 1 个）保留在 Tauri 层，不值得抽。

---

## Outstanding Issues

### P1: `commands/mod.rs` 仍 9,115 行

Action 7 的 `pub use` 只是 re-export，函数体没有移出去。需要真正拆分。

---

### P2: Doctor/Install prompt 结构重叠

~60% 内容重复。可考虑抽取 `prompts/common/tool-schema.md`。不急。

---

## Resolved Issues

| Issue | Resolution | Commit |
|-------|-----------|--------|
| Sessions domain inline parsing | 4 pure functions in `clawpal_core::sessions` | `de8fce4` |
| Cron domain inline parsing | 2 pure functions in `clawpal_core::cron` | `d47e550` |
| Watchdog domain inline parsing | `parse_watchdog_status` + `WatchdogStatus` struct in core | `bd697d9` |
| Backup/Upgrade domain parsing | 3 pure functions + 3 typed structs in `clawpal_core::backup` | `7554bd6` |
| Discord/Discovery domain parsing | 3 pure functions + 2 typed structs in `clawpal_core::discovery` | `64717b5` |

---

## Next Actions (for Codex)

### Action 1: 重做 `commands/mod.rs` 拆分（修复 `8fbe13d`）

**问题**：当前 `8fbe13d` commit 的子模块文件（`config.rs`, `sessions.rs` 等）只有 `pub use super::*` 一行 re-export，函数体仍全部留在 `mod.rs`（9,115 行）。这没有达到拆分目的。

**要求**：将函数体 **剪切（cut）** 到对应子文件，`mod.rs` 只保留：
- 共享 `use` 语句、类型定义、helper 函数
- `mod config; mod sessions; ...` 声明
- `pub use config::*; pub use sessions::*; ...` re-export

**具体操作**：

1. 对每个子模块文件（如 `commands/sessions.rs`）：
   - 将对应的 `pub async fn remote_*` 函数 **从 `mod.rs` 剪切到该文件**
   - 文件顶部加 `use super::*;` 引入 mod.rs 的共享依赖
   - 函数签名和逻辑 **不做任何修改**

2. `mod.rs` 中：
   - 删除已移走的函数体
   - 保留共享的 private helper（如 `remote_write_config_with_snapshot`、`remote_read_openclaw_config_text_and_json`、`remote_resolve_openclaw_config_path`、`SshConnectionPool` 相关代码）
   - 保留共享的 `use`、`struct`、`enum`、`impl` 定义
   - 如果 helper 太多，可放入 `commands/helpers.rs`

3. 分组对照（已有的子模块文件）：
   - `config.rs`: `remote_read_raw_config`, `remote_write_raw_config`, `remote_apply_config_patch`, `remote_list_history`, `remote_preview_rollback`, `remote_rollback`
   - `sessions.rs`: `remote_analyze_sessions`, `remote_delete_sessions_by_ids`, `remote_list_session_files`, `remote_preview_session`, `remote_clear_all_sessions`
   - `doctor.rs`: `remote_run_doctor`, `remote_fix_issues`, `remote_get_system_status`, `remote_get_status_extra`
   - `profiles.rs`: `remote_list_model_profiles`, `remote_upsert_model_profile`, `remote_delete_model_profile`, `remote_resolve_api_keys`, `remote_test_model_profile`, `remote_extract_model_profiles_from_config`
   - `watchdog.rs`: `remote_get_watchdog_status`, `remote_deploy_watchdog`, `remote_start_watchdog`, `remote_stop_watchdog`, `remote_uninstall_watchdog`
   - `cron.rs`: `remote_list_cron_jobs`, `remote_get_cron_runs`, `remote_trigger_cron_job`, `remote_delete_cron_job`
   - `backup.rs`: `remote_backup_before_upgrade`, `remote_list_backups`, `remote_restore_from_backup`, `remote_run_openclaw_upgrade`, `remote_check_openclaw_update`
   - `discovery.rs`: `remote_list_discord_guild_channels`, `remote_list_bindings`, `remote_list_channels_minimal`, `remote_list_agents_overview`
   - `logs.rs`: `remote_read_app_log`, `remote_read_error_log`, `remote_read_gateway_log`, `remote_read_gateway_error_log`
   - `rescue.rs`: `remote_manage_rescue_bot`, `remote_diagnose_primary_via_rescue`, `remote_repair_primary_via_rescue`
   - `gateway.rs`: `remote_restart_gateway`
   - `agent.rs`: `remote_setup_agent_identity`, `remote_chat_via_openclaw`

4. 未列入上述分组的其他 `pub` 函数（非 `remote_*` 的如 `list_registered_instances`、`create_install_session` 等）保留在 `mod.rs`。

5. 验证：
   - `cargo build --workspace` 通过
   - `cargo test --workspace --all-targets` 通过（remote_api 忽略）
   - `npx tsc --noEmit` 通过
   - `wc -l src-tauri/src/commands/mod.rs` 应显著低于 9,115（目标 < 4,000 行）

Commit message: `refactor: actually move function bodies into domain submodules`

**关键约束：不改任何函数签名或逻辑，只移动代码位置。**

---

## Execution History

| Batch | Status | Commits | Review Notes |
|-------|--------|---------|-------------|
| Batch E2: Sessions | **Done** | `de8fce4` | 4 pure functions, 4 tests, -237 lines from commands.rs |
| Batch E3: Cron | **Done** | `d47e550` | 2 pure functions, 2 tests, -51 lines from commands.rs |
| Batch E4: Watchdog | **Done** | `bd697d9` | 1 pure function + typed struct, 1 test, -21 lines from commands.rs |
| Batch E5: Backup/Upgrade | **Done** | `7554bd6` | 3 pure functions + 3 structs, 3 tests, -17 lines from commands.rs |
| Batch E6: Discord/Discovery | **Done** | `64717b5` | 3 pure functions + 2 structs, 3 tests, -116 lines from commands.rs |
| Quality verification | **Done** | `628f2c4` | All pass (remote_api env ignored), -290 lines total |
| commands.rs split (attempt 1) | **Redo** | `8fbe13d` | Only `pub use` stubs, mod.rs still 9,115 lines |
