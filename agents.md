# ClawPal 开发规范（agents.md）

## 1. 仓库约定

- 使用 Git 进行所有变更追踪
- 统一采用 UTF-8 编码
- 变更以原子提交为粒度，避免一次提交包含多个互不相关需求

## 2. 分支与 PR

- `main`: 受保护主线
- `feat/*`: 新功能（示例：`feat/recipe-preview`）
- `fix/*`: 缺陷修复（示例：`fix/rollback-edge-case`）
- `chore/*`: 工具/流程/文档维护

提交前确保：
- 运行相关的类型检查/构建脚本（如有）
- 更新相关文档（需要时）

## 3. 提交规范

使用 Conventional Commits：
- `feat:` 新功能
- `fix:` Bug 修复
- `docs:` 文档
- `refactor:` 重构
- `chore:` 维护

示例：
- `feat: add recipe preview diff panel`
- `fix: avoid duplicate snapshot id collisions`

## 4. 开发流程

每次变更建议按以下顺序执行：

1. 明确需求和验收标准
2. 先做最小实现
3. 自检关键流程（读取配置、预览、应用、回滚、Doctor）
4. 同步更新文档
5. 提交并标记未完成项

## 5. 代码质量要求

- 函数尽量短、职责单一
- 对外行为需具备错误返回，不抛出未处理异常
- 新增参数/结构体需有默认值或向后兼容路径
- 优先保持最小可运行状态再逐步演进

## 6. 任务追踪

建议在每轮开发前补充：
- 当前任务目标
- 预期验收项
- 完成后状态（完成 / 待验收）

可用文件：
- `docs/mvp-checklist.md`（验收）
- `docs/plans/2026-02-15-clawpal-mvp-design.md`（设计）
- `docs/plans/2026-02-15-clawpal-mvp-implementation-plan.md`（计划）

## 7. 部署

### 官网（clawpal.zhixian.io）

使用 Cloudflare Pages Direct Upload 部署，源目录为 `docs/site/`。

部署命令：
```bash
npx wrangler pages deploy docs/site --project-name clawpal
```

项目域名：`clawpal.zhixian.io`（也可通过 `clawpal.pages.dev` 访问）。

### 桌面应用 Release

通过 GitHub Actions 自动构建，push tag 触发（如 `v0.1.1`）：
- CI workflow: `.github/workflows/release.yml`
- 构建产物：macOS (ARM/x64 .dmg)、Windows (.exe/.msi)、Linux (.deb/.AppImage)
- 需要 `TAURI_SIGNING_PRIVATE_KEY` 等 secrets，本地无法打 release bundle
- 发布新版本流程：更新 `package.json` + `src-tauri/Cargo.toml` 版本号 → commit → `git tag vX.Y.Z` → push

## 8. 安全与风险

- 禁止提交明文密钥/配置路径泄露
- 避免大文件和自动生成产物直接提交
- 对 `~/.openclaw` 的读写逻辑需包含异常回退和用户可见提示
