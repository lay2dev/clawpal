# 版本发布流程

## 触发条件

需要发布新版本（正式或预发布）时。

## 排查步骤

1. 确认目标 commit 上所有 CI 通过
2. 确认相关 PR 已合并到 develop

## 发布流程

### 预发布（RC）

1. 从 develop 创建 RC 分支：
   ```bash
   git checkout develop
   git pull origin develop
   git checkout -b rc/vX.Y.Z-rc.N
   git push origin rc/vX.Y.Z-rc.N
   ```

2. `Bump Version` workflow 自动触发，更新版本号

3. 通过 GitHub Actions 手动触发 `Release` workflow：
   - `version`: 不带 `v` 前缀（如 `0.3.3-rc.1`）
   - `target_commitish`: RC 分支最新 commit SHA
   - `is_prerelease`: `true`

### 正式发布

1. 通过 GitHub Actions 手动触发 `Release` workflow：
   - `version`: 不带 `v` 前缀（如 `0.3.3`）
   - `target_commitish`: 目标 commit SHA
   - `is_prerelease`: `false`

2. 构建产物自动生成：
   - macOS (ARM/x64 .dmg)
   - Windows (.exe/.msi)
   - Linux (.deb/.AppImage)

## 常见原因（构建失败）

- 签名密钥缺失：检查 `TAURI_SIGNING_PRIVATE_KEY` secret
- 平台依赖变化：检查 CI runner 配置
- 版本号冲突：确认 `package.json` 和 `src-tauri/Cargo.toml` 版本一致

## 验证方法

- GitHub Releases 页面有完整产物
- 各平台安装包可正常安装启动
