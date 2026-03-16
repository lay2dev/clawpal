# 每周熵治理 Checklist

## 目标

防止仓库在高产能模式下失控。每周至少执行一次。

## Checklist

### 代码清理

- [ ] 删除无用代码和死分支
  ```bash
  git branch --merged develop | grep -v "main\|develop" | xargs git branch -d
  ```
- [ ] 合并重复实现（搜索相似函数名和逻辑）
- [ ] 清理 `TODO`、`FIXME`、`HACK` 注释
  ```bash
  grep -rn "TODO\|FIXME\|HACK" src/ src-tauri/src/ clawpal-core/src/
  ```

### 文档对齐

- [ ] `AGENTS.md` 是否与仓库实际结构一致
- [ ] `docs/architecture/` 是否反映最新模块划分
- [ ] `docs/runbooks/` 中的命令是否仍可执行
- [ ] `Makefile` 中的命令是否仍有效

### 归档

- [ ] 归档 `docs/plans/` 中已完成的任务计划（移入 `docs/plans/archived/` 或标记状态）
- [ ] 关闭已解决的 GitHub Issues

### 依赖

- [ ] 检查 Rust 依赖是否有安全更新
  ```bash
  cargo audit    # 需安装 cargo-audit
  ```
- [ ] 检查前端依赖是否有安全更新
  ```bash
  bun audit
  ```

### Agent 失败复盘

- [ ] 本周 agent 产出的 PR 中，有多少需要人工修正？
- [ ] 失败原因是什么？（harness 问题 vs 模型问题）
- [ ] 能否转化为新的规则、lint 或 runbook？

### 指标记录

| 指标 | 本周 | 上周 | 趋势 |
|------|------|------|------|
| PR 中位生命周期 | | | |
| 单 PR 平均变更行数 | | | |
| Agent 独立完成任务占比 | | | |
| 回退/返工率 | | | |
| CI 失败中环境问题占比 | | | |
| 同类问题重复出现次数 | | | |

## 执行建议

- 每周一或周五固定时间
- 指定一人负责（可轮值）
- 结果记录到 `docs/plans/` 或 issue 中
