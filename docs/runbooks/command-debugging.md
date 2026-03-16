# Tauri Command 调用失败排查

## 触发条件

前端调用 Tauri command 返回错误或无响应。

## 排查步骤

1. 打开 DevTools (Ctrl+Shift+I / Cmd+Option+I)
2. 检查 Console 中的 invoke 错误信息
3. 检查 Rust 侧日志输出（终端或日志文件）
4. 确认 command 是否在 `invoke_handler!` 中注册
5. 确认参数类型前后端是否匹配

## 常见原因

- Command 未注册到 `invoke_handler!`
- 前后端参数类型不一致（特别是 camelCase vs snake_case）
- Tauri 权限/capability 未配置
- Command 内部 panic（检查 Rust 日志）

## 修复动作

- 注册缺失：在 `lib.rs` 的 `invoke_handler!` 宏中添加
- 类型不一致：检查 `#[tauri::command]` 参数与前端 invoke 调用
- 权限缺失：更新 `src-tauri/capabilities/`

## 修复后验证

```bash
make lint           # 确保类型和格式正确
make test-unit      # 确保没有引入回归
```

DevTools Console 中 invoke 调用返回预期结果，无错误。
