# ClawPal Harness Engineering 标准落地计划

关联 Issue: https://github.com/lay2dev/clawpal/issues/123

## 目标

将 ClawPal 仓库从当前状态改造为符合 Harness Engineering 标准的 agent-first 工程仓库。

## 非目标

- 不做产品功能重设计
- 不做大规模代码重写（Phase 3 拆分除外）
- 不切换技术栈

## 执行阶段

### Phase 1: 仓库入口归一（本 PR）

- [x] `agents.md` → `AGENTS.md`，按标准补全内容
- [x] 建立 `docs/architecture/` 并迁移 `design.md`
- [x] 建立 `docs/decisions/` 并迁移 `cc*.md`
- [x] 建立 `docs/runbooks/` 并创建初始 runbook
- [x] 建立 `harness/fixtures/` 和 `harness/artifacts/`

### Phase 2: 验证与流程归一

独立 PR。

- [ ] 落地 `justfile`，统一 dev/test/lint/smoke/package 命令
- [ ] 统一包管理器策略（Bun vs npm）
- [ ] 增加 PR 模板 (`.github/PULL_REQUEST_TEMPLATE.md`)
- [ ] 增加 issue 模板
- [ ] 将 `business-flow-test-matrix.md` 升级为标准 gate 文档
- [ ] 补 packaged app smoke test 入口
- [ ] 补 artifacts 汇总命令

### Phase 3: 代码可读性改造

多个独立 PR。

- [ ] 拆分 `src/App.tsx`（约 1,787 行）为路由/功能模块
- [ ] 拆分 `src-tauri/src/commands/mod.rs`（约 10,546 行）为领域模块
- [ ] 收口 GUI / core / remote helper 边界
- [ ] 为高风险模块补 `docs/architecture/<module>.md`
- [ ] 补 command contract tests

### Phase 4: 机制固化

独立 PR。

- [ ] CI gate 强制 PR 验证证据
- [ ] 关键目录加 CODEOWNERS
- [ ] 高风险调用链加约束测试
- [ ] Runbook 增加失败诊断和回滚路径
- [ ] 建立每周熵治理 checklist

## 验收标准

- Agent 能在 30 分钟内通过 `AGENTS.md` 独立启动项目
- 所有验证命令通过 `justfile` 一站式入口调用
- 关键模块有 architecture note
- PR 有统一模板和证据要求

## 风险与回滚

- 文档迁移可能导致外部链接失效 → 在原位置留 redirect 注释
- 代码拆分可能引入回归 → 每次拆分独立 PR + 完整 CI
