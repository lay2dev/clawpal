# ClawPal Harness Engineering 标准落地计划

关联 Issue: https://github.com/lay2dev/clawpal/issues/123

## 目标

将 ClawPal 仓库从当前状态改造为符合 Harness Engineering 标准的 agent-first 工程仓库。

## 非目标

- 不做产品功能重设计
- 不做大规模代码重写（Phase 3 拆分除外）
- 不切换技术栈

## 执行阶段

### Phase 1: 仓库入口归一 ✅

PR #124 (merged)

- [x] `agents.md` → `AGENTS.md`，按标准补全内容
- [x] 建立 `docs/architecture/` 并迁移 `design.md`
- [x] 建立 `docs/decisions/` 并迁移 `cc*.md`
- [x] 建立 `docs/runbooks/` 并创建初始 runbook
- [x] 建立 `harness/fixtures/` 和 `harness/artifacts/`

### Phase 2: 验证与流程归一 ✅

PR #125 (merged)

- [x] 落地 `Makefile`，统一 dev/test/lint/smoke/package 命令
- [x] 增加 PR 模板 (`.github/PULL_REQUEST_TEMPLATE.md`)
- [x] 增加 issue 模板（bug report、feature request、task）
- [x] 补 artifacts 汇总命令（`make artifacts`）
- [x] ADR-001: Makefile vs justfile 决策记录

### Phase 3: 代码可读性改造 ✅

PR #126 (merged) + PR #127

- [x] 拆分 `src-tauri/src/commands/mod.rs` — 52 个 tauri command 提取到 9 个领域子模块
- [x] 为高风险模块补 `docs/architecture/` 说明（overview.md, commands.md）
- [x] 将 `business-flow-test-matrix.md` 升级为标准 gate 文档（6 级 gate 定义）

**已明确延后（需独立 PR）**:
- [ ] 拆分 `src/App.tsx`（1,787 行，79 个 hooks，需前端专项重构）
- [ ] 继续拆分 `mod.rs` 中的 `remote_*` 代理命令
- [ ] 补 command contract tests
- [ ] 统一 Bun/npm 策略（CI 混用 `bun install` / `npm ci`）

### Phase 4: 机制固化 ✅

PR #127

- [x] 关键目录加 CODEOWNERS
- [x] Runbook: 故障诊断与回滚路径（`docs/runbooks/failure-diagnosis.md`）
- [x] 建立每周熵治理 checklist（`docs/runbooks/entropy-governance.md`）

**已明确延后（需独立 PR）**:
- [ ] CI gate 强制 PR 验证证据（需修改 workflow yaml）
- [ ] 高风险调用链加约束测试（需 Rust 代码改动）
- [ ] 补 packaged app smoke test 入口

## 验收标准

| 标准 | 状态 |
|------|------|
| Agent 能通过 `AGENTS.md` 独立启动项目 | ✅ |
| 所有验证命令通过 `Makefile` 一站式入口调用 | ✅ |
| 关键模块有 architecture note | ✅ |
| PR 有统一模板和证据要求 | ✅ |
| 文档目录结构完整（architecture/decisions/runbooks/plans/testing） | ✅ |
| 代码所有者明确 | ✅ |
| 测试矩阵有标准 gate 定义 | ✅ |
| 熵治理有固定流程 | ✅ |

## 风险与回滚

- 文档迁移可能导致外部链接失效 → 已在原位置留 redirect 文件
- 代码拆分可能引入回归 → 每次拆分独立 PR + 完整 CI

## 延后项跟踪

以下工作项已明确延后，建议作为独立 issue/PR 推进：

1. **App.tsx 拆分** — 1,787 行、79 个 hooks，需要前端专项重构计划
2. **remote_* 命令拆分** — mod.rs 仍有 ~8,800 行，主要是 remote 代理和共享类型
3. **Command contract tests** — 为每个 tauri command 补 I/O 契约测试
4. **Bun/npm 统一** — CI 中 `pr-build.yml` 和 `release.yml` 仍用 `npm ci`
5. **CI 证据 gate** — 强制 PR 附带测试截图/日志
6. **Packaged app smoke test** — 打包后的冒烟验证入口
