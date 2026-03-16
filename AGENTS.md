# AGENTS.md

ClawPal 是基于 Tauri 的 OpenClaw 桌面伴侣应用，覆盖安装、配置、Doctor 诊断、版本回滚、远程 SSH 管理和多平台打包发布。

技术栈：Tauri v2 + Rust + React + TypeScript + Bun

## 目录说明

```
src/                          # 前端（React/TypeScript）
src/lib/api.ts                # 前端对 Tauri command 的统一封装
src-tauri/src/commands/       # Tauri command 层（参数校验、权限检查、错误映射）
src-tauri/src/commands/mod.rs # Command 路由与公共逻辑
clawpal-core/                 # 核心业务逻辑（与 Tauri 解耦）
clawpal-cli/                  # CLI 接口
docs/architecture/            # 模块边界、分层原则、核心数据流
docs/decisions/               # 关键设计决策（ADR）
docs/plans/                   # 任务计划与实施方案
docs/runbooks/                # 启动、调试、发布、回滚、故障处理
docs/testing/                 # 测试矩阵与验证策略
harness/fixtures/             # 最小稳定测试数据
harness/artifacts/            # 日志、截图、trace、失败产物收集
```

## 启动命令

```bash
bun install               # 安装前端依赖
bun run dev:tauri           # 启动开发模式（前端 + Tauri）
bun run dev                # 仅启动前端
cargo test --workspace     # Rust 单元测试
bun run test               # 前端单元测试
bun run typecheck          # TypeScript 类型检查
cargo fmt --check          # Rust 格式检查
cargo clippy               # Rust lint
```

## 代码分层约束

### UI 层 (`src/`)
- 不直接在组件中使用 `invoke("xxx")`，通过 `src/lib/api/` 封装调用
- 不直接访问原生能力
- 不拼接 command 名称和错误字符串

### Command 层 (`src-tauri/src/commands/`)
- 保持薄层：参数校验、权限检查、错误映射、事件分发
- 不堆积业务编排逻辑
- 不直接写文件系统或数据库

### Domain 层 (`clawpal-core/`)
- 核心业务规则和用例编排
- 尽量不依赖 `tauri::*`
- 输入输出保持普通 Rust 类型

### Adapter 层
- 所有原生副作用（文件系统、shell、通知、剪贴板、updater）从 adapter 层进入
- 须提供测试替身（mock/fake）

## 提交与 PR 要求

- Conventional Commits: `feat:` / `fix:` / `docs:` / `refactor:` / `chore:`
- 分支命名: `feat/*` / `fix/*` / `chore/*`
- PR 变更建议 ≤ 500 行（不含自动生成文件）
- PR 必须通过所有 CI gate
- 涉及 UI 改动须附截图
- 涉及权限/安全改动须附 capability 变更说明

## 新增 Command 检查清单

- [ ] Command 定义在 `src-tauri/src/commands/` 对应模块
- [ ] 参数校验和错误映射完整
- [ ] 已在 `lib.rs` 的 `invoke_handler!` 中注册
- [ ] 前端 API 封装已更新
- [ ] 相关文档已更新

## 安全约束

- 禁止提交明文密钥或配置路径泄露
- Command 白名单制，新增原生能力必须补文档和验证
- 对 `~/.openclaw` 的读写需包含异常回退和用户可见提示
- 默认最小权限原则

## 常见排查路径

- **Command 调用失败** → 见 `docs/runbooks/command-debugging.md`
- **本地开发启动** → 见 `docs/runbooks/local-development.md`
- **版本发布** → 见 `docs/runbooks/release-process.md`
- **打包后行为与 dev 不一致** → 检查资源路径、权限配置、签名、窗口事件
- **跨平台差异** → 检查 adapter 层平台分支和 CI 构建日志

## 参考文档

- [Harness Engineering 标准](https://github.com/lay2dev/clawpal/issues/123)
- [落地计划](docs/plans/2026-03-16-harness-engineering-standard.md)
- [架构设计](docs/architecture/design.md)
- [测试矩阵](docs/testing/business-flow-test-matrix.md)
