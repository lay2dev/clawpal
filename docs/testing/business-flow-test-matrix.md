# Business Flow Test Matrix

## Goal

After GUI-CLI-Core layering, business logic verification is core/CLI-first, with GUI focused on integration and UX wiring.

## Gate 定义

### Gate 1: Fast Local Gate（提交前必须通过）

```bash
make test-unit    # 等价于以下命令：
# cargo test -p clawpal-core
# cargo test -p clawpal-cli
# bun test
```

**验收标准**: 全部测试通过，无 panic，无 warning。

### Gate 2: Extended Local Gate（合并前推荐）

```bash
cargo test -p clawpal --test install_api --test runtime_types --test commands_delegation
cargo run -p clawpal-cli -- instance list
cargo run -p clawpal-cli -- ssh list
cargo test -p clawpal --test wsl2_runner   # 非 Windows 上跑 placeholder
```

**验收标准**: 所有 API 集成测试通过，CLI 命令正常返回。

### Gate 3: CI Gate（PR 合并条件）

由 `.github/workflows/ci.yml` 自动执行：

| 检查项 | 命令 | 阻断级别 |
|--------|------|----------|
| 前端类型检查 | `bun run typecheck` | 必须通过 |
| 前端构建 | `bun run build` | 必须通过 |
| Rust 格式 | `cargo fmt --check` | 必须通过 |
| Rust lint | `cargo clippy -p clawpal-core -- -D warnings` | 必须通过 |
| Rust 单元测试 | `cargo test -p clawpal-core` | 必须通过 |
| 覆盖率 | `cargo llvm-cov` | 必须通过（不得下降） |
| Profile E2E | profile 创建/编辑/删除 | 必须通过 |
| 多平台构建 | macOS ARM64/x64, Windows x64, Linux x64 | 必须通过 |

### Gate 4: Remote Gate（需要可达的 `vm1`）

```bash
cargo test -p clawpal --test remote_api -- --test-threads=1
```

**备注**: 4 个测试被 `ignored`（手动/可选）。需要 SSH 到 `vm1` 的网络连通性。

### Gate 5: Optional Docker Gate（本地机器）

```bash
CLAWPAL_RUN_DOCKER_LIVE_TESTS=1 cargo test -p clawpal-core --test docker_live -- --nocapture
```

**备注**: 端口 `18789` 被占用时自动跳过。

### Gate 6: Optional WSL2 Gate（仅 Windows）

```bash
cargo test -p clawpal --test wsl2_runner -- --ignored
```

## Layer Ownership

| 层 | 职责 | 测试重点 |
|----|------|----------|
| `clawpal-core` | 业务规则、持久化、SSH 注册、安装/连接/健康逻辑 | 单元测试 + 集成测试 |
| `clawpal-cli` | JSON contract、命令路由 | Contract 测试 |
| `src-tauri` | 薄 command 委派、状态绑定、运行时事件桥接 | 编译检查 + E2E |
| Frontend GUI | 用户交互、渲染、invoke 审批 UX | 类型检查 + 构建 |

## 回归优先级

1. **实例注册一致性** — `instances.json`（local/docker/remote ssh）
2. **SSH 读写正确性** — 远程命令错误必须显式失败
3. **Docker 安装行为** — 阻止 no-op 回归
4. **Doctor 工具契约** — 仅限 `clawpal`/`openclaw`
