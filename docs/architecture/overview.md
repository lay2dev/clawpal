# ClawPal 架构概览

## 系统定位

ClawPal 是基于 Tauri v2 的 OpenClaw 桌面伴侣应用，提供安装、配置、诊断、回滚、远程管理等功能的图形化界面。

## 技术栈

- **前端**: React + TypeScript + Vite
- **桌面框架**: Tauri v2
- **后端**: Rust (Tauri commands + clawpal-core + clawpal-cli)
- **包管理**: Bun (前端) + Cargo (Rust)

## 分层架构

```
┌────────────────────────────────────────┐
│            UI 层 (src/)                │
│  React 组件 + 状态管理 + 路由          │
│  API 封装: src/lib/api.ts             │
├────────────────────────────────────────┤
│       Command 层 (src-tauri/src/commands/) │
│  Tauri command 定义                    │
│  参数校验 · 权限检查 · 错误映射        │
├────────────────────────────────────────┤
│       Domain 层 (clawpal-core/)        │
│  核心业务逻辑（与 Tauri 解耦）          │
│  SSH · Doctor · Config · Install       │
├────────────────────────────────────────┤
│         CLI 层 (clawpal-cli/)          │
│  命令行接口                            │
└────────────────────────────────────────┘
```

## 代码目录

### 前端 (`src/`)

| 目录/文件 | 职责 |
|-----------|------|
| `App.tsx` | 主应用组件（路由、实例管理、全局状态） |
| `pages/` | 页面组件（Home, Settings, Doctor, Recipes 等） |
| `components/` | 共享组件 |
| `lib/api.ts` | Tauri command 调用封装 |
| `lib/` | 工具函数、hooks、类型定义 |

### Tauri Command 层 (`src-tauri/src/commands/`)

| 模块 | 命令数 | 领域 |
|------|--------|------|
| `agent.rs` | 6 | Agent 管理 |
| `backup.rs` | 11 | 备份/恢复 |
| `config.rs` | 11 | 配置读写 |
| `cron.rs` | 8 | 定时任务 |
| `discovery.rs` | 10 | 实例发现 |
| `doctor.rs` | 11 | 诊断修复 |
| `doctor_assistant.rs` | 4 | Doctor AI 助手 |
| `gateway.rs` | 2 | Gateway 管理 |
| `instance.rs` | 13 | 实例连接/注册 |
| `logs.rs` | 5 | 日志查看 |
| `model.rs` | 6 | 模型配置 |
| `overview.rs` | 12 | 概览/状态 |
| `precheck.rs` | 4 | 预检查 |
| `preferences.rs` | 7 | 偏好设置 |
| `profiles.rs` | 20 | 模型 Profile |
| `rescue.rs` | 4 | 救援机器人 |
| `sessions.rs` | 10 | 会话管理 |
| `ssh.rs` | 15 | SSH/SFTP |
| `watchdog.rs` | 5 | 看门狗（原有） |
| `watchdog_cmds.rs` | 5 | 看门狗命令 |
| `app_logs.rs` | 6 | 应用日志 |
| `upgrade.rs` | 1 | 升级 |
| `recipe_cmds.rs` | 1 | 配方 |
| `util.rs` | 1 | 工具 |
| `mod.rs` | — | 共享类型 + remote_* 代理 |

### Domain 层 (`clawpal-core/src/`)

| 模块 | 职责 |
|------|------|
| `config.rs` | 配置解析与管理 |
| `connect.rs` | 连接管理 |
| `doctor.rs` | 诊断引擎 |
| `health.rs` | 健康检查 |
| `instance.rs` | 实例模型 |
| `ssh/` | SSH 连接、诊断、传输 |
| `install/` | 安装流程编排 |
| `profile.rs` | 模型 Profile |
| `watchdog.rs` | 看门狗逻辑 |

## 关键数据流

### 本地实例管理

```
UI (App.tsx) → api.ts → invoke("connect_local_instance")
  → commands/instance.rs → clawpal-core/connect.rs
  → 读取 ~/.openclaw/config.yaml → 返回实例状态
```

### SSH 远程管理

```
UI → api.ts → invoke("ssh_connect")
  → commands/ssh.rs → SshConnectionPool
  → OpenSSH 子进程 → 远程主机
```

### Doctor 诊断

```
UI (Doctor 页面) → api.ts → invoke("run_doctor_command")
  → commands/doctor.rs → clawpal-core/doctor.rs
  → 执行诊断规则 → 返回 DoctorReport
```

## 约束规则

见 [AGENTS.md](../../AGENTS.md) 的代码分层约束部分。
