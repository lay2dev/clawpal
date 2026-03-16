# Code Review Notes (Claude → Codex)

Last updated: 2026-02-28

This file contains review findings and action items from architecture audits. Codex should check this file periodically and work through the items.

## Codex Feedback

Last run: 2026-02-28

| Action | Status | Result |
|--------|--------|--------|
| Review Action 1: 修复两个测试失败 | PASS | install prompt 已补充 `doctor exec --tool <command> [--args <argstring>] [--instance <id>]`；`tool_intent::classify_invoke_type` 在 openclaw 非写操作分支返回 `read`。验证：`cargo test --workspace --all-targets` 除 `remote_api` 环境限制（`192.168.65.2:22 Operation not permitted`）外通过。提交：`c457bcc` |
| Review Action 2: 去除 SSH 去重冗余 | PASS | 已移除 `commands/mod.rs::list_registered_instances` 的 `seen_remote` 去重和 `StartPage.tsx` 的 `seenSshEndpoints` 去重，统一信任 `clawpal-core/src/ssh/registry.rs`。验证：`cargo build --workspace`、`npx tsc --noEmit` 通过；`cargo test --workspace --all-targets` 仅 `remote_api` 环境限制失败。提交：`51408c8` |
| Action 1: Batch E2 Sessions | PASS | 新增 `clawpal-core/src/sessions.rs`，迁移 `remote_analyze_sessions` / `remote_delete_sessions_by_ids` / `remote_list_session_files` / `remote_preview_session` 的纯解析与过滤逻辑到 core（`parse_session_analysis`、`filter_sessions_by_ids`、`parse_session_file_list`、`parse_session_preview`）；Tauri 端改为调用 core。新增 4 个 core 单测并通过。 |
| Action 2: Batch E3 Cron | PASS | 新增 `clawpal-core/src/cron.rs`，迁移 `parse_cron_jobs` / `parse_cron_runs`；`commands.rs` 本地与远端 cron 读取路径改为调用 core 解析。新增 2 个 core 单测并通过。 |
| Action 3: Batch E4 Watchdog | PASS | 新增 `clawpal-core/src/watchdog.rs`，迁移 watchdog 状态合并判断到 `parse_watchdog_status`；`remote_get_watchdog_status` 改为调用 core 解析后补充 `deployed`。新增 1 个 core 单测并通过。 |
| Action 4: Batch E5 Backup/Upgrade | PASS | 新增 `clawpal-core/src/backup.rs`，迁移 `parse_backup_list` / `parse_backup_result` / `parse_upgrade_result`；`remote_backup_before_upgrade` 与 `remote_list_backups` 改为调用 core 解析，`remote_run_openclaw_upgrade` 接入升级输出解析。新增 3 个 core 单测并通过。 |
| Action 5: Batch E6 Discord/Discovery | PASS | 新增 `clawpal-core/src/discovery.rs`，迁移 Discord guild/channel 与 bindings 解析（`parse_guild_channels`、`parse_bindings`）及绑定合并函数（`merge_channel_bindings`）。`remote_list_discord_guild_channels` 与 `remote_list_bindings` 已改为优先调用 core 解析，保留原 SSH/REST fallback。新增 3 个 core 单测并通过。 |
| Action 6: 质量验证 | PASS (remote_api ignored) | `cargo build --workspace` 通过；`npx tsc --noEmit` 通过；`cargo test --workspace --all-targets` 仅 `remote_api` 因 `192.168.65.2:22 Operation not permitted` 失败，按说明忽略。`commands.rs` 行数：`9367 -> 9077`（减少 `290` 行）。 |
| Action 7: commands.rs 拆文件 | PASS | remote_* 函数体移入 12 个子模块，mod.rs 9115→6005 行（剩余为本地操作 + 共享 helper）。build/test/tsc 通过。 |
| Review Action 3: SSH 泄漏修复（disconnect/connect timeout + sftp_write 复用连接） | PASS | `clawpal-core/src/ssh/mod.rs`：3 处 `handle.disconnect` 增加 3s timeout；`connect_and_auth` 增加 10s timeout；`sftp_write` 去除 `self.exec(mkdir)` 额外连接，改为同 handle 新 channel 执行 `mkdir -p`。`cargo build --workspace` 通过；`cargo test --workspace --all-targets` 仅 `remote_api` 环境限制失败。提交：`d515772` |
| Review Action 4: Doctor 任意命令执行链路 | PASS | prompt + 后端联动支持 `doctor exec --tool/--args`，并在 `tool_intent` 标记为 write，保持审批路径一致。`cargo build --workspace`、`npx tsc --noEmit` 通过。提交：`b360fb1` |
| Review Action 5: 频道缓存上提 | PASS | `InstanceContext/useApi/Channels` 统一使用 app 级缓存与 loading 状态，减少重复拉取；`ParamForm` 兼容 `null` 缓存。`cargo build --workspace`、`npx tsc --noEmit` 通过。提交：`e90e4a3` |
| Review Action 6: 启动与 UI 行为修复 | PASS | 启动 splash（`index.html/main.tsx`）、SSH registry endpoint 去重、Cron 红点改为“按时运行”判定（5 分钟宽限）、Doctor 启动携带小龙虾上下文、Home 重复安装提示改走小龙虾。`cargo build --workspace`、`npx tsc --noEmit` 通过。提交：`56800e4`、`b7a55dd`、`83ee6c2` |

---

## Context

三层架构重构（Phase 1-10）已完成，见 `cc-architecture-refactor-v1.md`。

本轮目标：将 `commands.rs` 中剩余 `remote_*` 函数按领域迁移到 `clawpal-core`。

当前 `commands.rs`：9,367 行，41 个 `remote_*` 函数。其中约 20 个已部分调用 core，约 21 个纯 inline SFTP+JSON。

迁移原则：只迁移有实际 JSON 解析/操作逻辑的函数。纯薄包装（Logs 4 个、Gateway 1 个、Agent Setup 1 个）保留在 Tauri 层，不值得抽。

---

## Outstanding Issues

---

### P1: `run_doctor_exec_tool` 安全审查

`doctor_commands.rs` 新增的 `run_doctor_exec_tool` 允许在 host 上执行任意命令（`std::process::Command::new(command)`）。虽然 UI 有确认步骤（tool_intent 分类为 `"write"`），但 `validate_payload` 现在只检查 `tool.is_empty()`，不再限制 tool name。需确保：
- prompt 不会被注入绕过确认流程
- 考虑是否需要命令白名单或黑名单（至少禁止 `rm`、`dd` 等破坏性命令）

当前状态：**有意设计，但需要确认安全策略是否足够**。

---

### P2: `commands/mod.rs` 仍 6,005 行

已从 9,115 降到 6,005（remote_* 函数体已移出）。剩余为本地操作 + 共享 helper，进一步拆分属于下一轮优化。

---

### P3: Doctor/Install prompt 结构重叠

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
| commands.rs split into domain modules | remote_* moved to 12 submodules, mod.rs 9115→6005 | `8fbe13d`, `ed1a8f2` |
| Missed WIP + housekeeping | session_scope, tool_intent mod, i18n.language, gitignore | `3292982` |

---

## Next Actions (for Codex)

（当前无阻塞性 action。P0 SSH 泄漏已解决，所有 review action 已完成。）

### 可选优化

- `refresh_session()` 连续重连失败时加 backoff（当前 semaphore 2/host 已限制并发，不急）
- P2: `commands/mod.rs` 进一步拆分（6,005 行 → 按本地操作领域拆）
- P3: Doctor/Install prompt 去重

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
| commands.rs split (attempt 2) | **Done** | `ed1a8f2` | Functions moved to 12 submodules, mod.rs 9115→6005 |
| Housekeeping | **Done** | `3292982` | WIP commit + gitignore + archive |
| SSH session reuse pool (P0) | **Done** | `46b2509` | persistent handle per host, cooldown removed, auto-retry on stale |
| Login shell unification | **Done** | `0f3c88f`, `0235e38` | wrap_login_shell_wrapper, -ilc for zsh/bash |
| Frontend perf (lazy load + transitions) | **Done** | `9e418a2`, `a15533a` | React.lazy 11 modules, startTransition, spawn_blocking for status |
| SSH error UX | **Done** | `ba08aed`, `a7864e3` | suppress transient channel errors, avoid re-explaining |
