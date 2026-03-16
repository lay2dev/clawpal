# Command 层架构

## 职责

`src-tauri/src/commands/` 是 Tauri command 层，负责：

1. 定义 `#[tauri::command]` 函数
2. 参数校验与反序列化
3. 权限和状态检查
4. 调用 domain 层逻辑
5. 错误映射为前端可用格式
6. 事件分发（`app.emit()`）

## 结构

```
commands/
├── mod.rs              # 共享类型/常量/helpers + remote_* 代理命令
├── agent.rs            # Agent CRUD
├── app_logs.rs         # 应用日志读取
├── backup.rs           # 备份/恢复
├── config.rs           # 配置读写
├── cron.rs             # 定时任务
├── discover_local.rs   # 本地实例发现
├── discovery.rs        # 实例发现（通用）
├── doctor.rs           # 诊断修复
├── doctor_assistant.rs # Doctor AI 助手
├── gateway.rs          # Gateway 管理
├── instance.rs         # 实例连接/注册/管理
├── logs.rs             # 日志查看
├── model.rs            # 模型/通道配置
├── overview.rs         # 概览/状态查询
├── precheck.rs         # 安装预检查
├── preferences.rs      # 偏好设置
├── profiles.rs         # 模型 Profile 管理
├── recipe_cmds.rs      # 配方列表
├── rescue.rs           # 救援机器人
├── sessions.rs         # 会话管理
├── ssh.rs              # SSH/SFTP 操作
├── upgrade.rs          # OpenClaw 升级
├── util.rs             # 工具函数
├── watchdog.rs         # 看门狗（原有模块）
└── watchdog_cmds.rs    # 看门狗部署/管理命令
```

## 模块组织原则

- 每个模块以 `use super::*;` 继承 `mod.rs` 的共享导入
- `mod.rs` 通过 `pub use <module>::*;` 重新导出所有命令
- `lib.rs` 的 `invoke_handler!` 使用 glob import，新增模块无需修改

## 新增 Command 流程

1. 在对应领域模块中添加 `#[tauri::command]` 函数
2. 如果是新模块：在 `mod.rs` 中添加 `pub mod <name>;` 和 `pub use <name>::*;`
3. 在 `lib.rs` 的 `invoke_handler!` 宏中注册函数名
4. 更新前端 `src/lib/api.ts` 中的调用封装
5. 运行 `make lint` 和 `make test-unit` 验证

## remote_* 代理命令

`mod.rs` 中保留大量 `remote_*` 前缀的函数，它们通过 SSH 在远程实例上执行对应的本地命令。这些函数共享一套 SSH 连接和序列化基础设施，因此暂保留在 `mod.rs` 中。

## 禁止事项

- 不在 command 层堆积业务逻辑 — 编排逻辑放 domain 层
- 不直接操作文件系统 — 通过 domain 层或 adapter
- 不在 command 函数中 panic — 所有错误通过 `Result<T, String>` 返回
