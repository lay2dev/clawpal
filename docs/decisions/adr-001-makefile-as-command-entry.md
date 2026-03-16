# ADR-001: 使用 Makefile 作为统一命令入口

## 状态

已采纳 (2026-03-16)

## 背景

Harness Engineering 标准要求项目有一个固定的、可发现的命令入口，让工程师和 coding agent 能通过统一命令完成开发、测试、构建和验证。

[tauri-harness-system-design.md](https://github.com/Keith-CY/harness-framework/blob/investigation/docs/tauri-harness-system-design.md) 建议使用 `justfile` 或 `cargo xtask`。

## 候选方案对比

| 维度 | Makefile | justfile | cargo xtask | package.json scripts | Shell 脚本 |
|------|----------|----------|-------------|---------------------|-----------|
| 安装成本 | 零（macOS/Linux 自带） | 需单独安装 | 需编写 Rust 代码 | 零 | 零 |
| 跨语言支持 | ✅ 任意命令 | ✅ 任意命令 | 偏 Rust | 偏 Node | ✅ 任意命令 |
| 命令依赖 | 原生支持 | 原生支持 | 需手写 | 不支持 | 需手写 |
| Agent 可读性 | 高（固定格式） | 高 | 中 | 中 | 中 |
| 生态惯例 | Rust 大项目常见 | 新兴 | Rust 专用 | Node 标配 | 通用 |
| 已知缺点 | tab 缩进强制、`$$` 转义 | 需安装 | 开发成本高 | 无法覆盖 Rust | 需 chmod/shebang |

## 决策

采用 **Makefile**。

## 理由

1. **零安装成本** — 不要求开发者安装额外工具
2. **ClawPal 是 TypeScript + Rust 混合项目** — `package.json scripts` 管不到 Rust 侧，`cargo xtask` 管不到前端，`Makefile` 两边都能覆盖
3. **命令依赖是天然的** — `ci: lint test-unit build` 一行定义完整 CI 链路
4. **Rust 生态惯例** — tokio、serde 等大型 Rust 项目广泛使用 Makefile
5. **Agent 友好** — 固定格式，target 名即命令，`make help` 自发现

## 后果

- 贡献者需注意 Makefile 使用 tab 缩进（不是空格）
- Windows 开发者需通过 Git Bash 或 WSL 使用 `make`（CI 均在 Linux/macOS 上运行，影响有限）
