# 版本发布流程

## 触发条件

需要发布新版本（正式或预发布）时。

## 前置条件

- 目标 commit 上所有 CI 通过
- 相关 PR 已合并

## 发布流程

### 预发布（RC）

1. 从 develop 创建 RC 分支：
   ```bash
   git checkout develop
   git pull origin develop
   git checkout -b rc/vX.Y.Z-rc.N
   git push origin rc/vX.Y.Z-rc.N
   ```

2. 推送 RC 分支后自动触发：
   - `Bump Version` workflow 检测 `rc/v*` 分支，自动计算并提交版本号
   - 版本提交完成后，`Bump Version` 自动 dispatch `Release` workflow
   - `Release` workflow 创建/更新 draft release 并构建全平台产物

无需手动触发任何 workflow。

### 正式发布

1. 从 main 创建 RC 分支：
   ```bash
   git checkout main
   git pull origin main
   git checkout -b rc/vX.Y.Z
   git push origin rc/vX.Y.Z
   ```

2. 同样自动触发 `Bump Version` → `Release` 链路。

### 手动触发（特殊情况）

如需手动控制版本号，可通过 GitHub Actions 手动触发 `Bump Version` workflow：
- `bump_type`: 选择 `patch` / `minor` / `major` / `custom`
- `custom_version`: 自定义版本号（仅 `custom` 时使用）

## 构建产物

- macOS ARM64 (.dmg)
- macOS x64 (.dmg)
- Windows x64 (.exe / .msi)
- Linux x64 (.deb / .AppImage)

## 常见原因（构建失败）

- 签名密钥缺失：检查 `TAURI_SIGNING_PRIVATE_KEY` secret
- 版本号冲突：`Bump Version` 会自动同步 `package.json` 和 `src-tauri/Cargo.toml`
- 平台依赖变化：检查 CI runner 配置

## 验证方法

- GitHub Releases 页面有完整 draft release 和产物
- 各平台安装包可正常安装启动
