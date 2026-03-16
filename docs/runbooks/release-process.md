# 版本发布流程

## 触发条件

需要发布新版本时。

## 排查步骤

1. 确认所有 CI 通过
2. 确认 CHANGELOG 已更新

## 发布流程

1. 更新版本号：
   - `package.json` → `version`
   - `src-tauri/Cargo.toml` → `version`
   - `src-tauri/tauri.conf.json` → `version`

2. 提交版本变更：
   ```bash
   git commit -am "chore: bump version to vX.Y.Z"
   ```

3. 打 tag 并推送：
   ```bash
   git tag vX.Y.Z
   git push origin develop --tags
   ```

4. GitHub Actions 自动构建：
   - macOS (ARM/x64 .dmg)
   - Windows (.exe/.msi)
   - Linux (.deb/.AppImage)

## 常见原因（构建失败）

- 签名密钥缺失：检查 `TAURI_SIGNING_PRIVATE_KEY` secret
- 平台依赖变化：检查 CI runner 配置

## 验证方法

- GitHub Releases 页面有完整产物
- 各平台安装包可正常安装启动
