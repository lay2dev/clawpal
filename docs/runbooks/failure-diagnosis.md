# 故障诊断与回滚

## 触发条件

生产环境或 CI 中出现非预期错误，需要定位原因并决定是否回滚。

## 诊断流程

### Step 1: 确认影响范围

- 哪个平台？（macOS / Windows / Linux）
- 哪个功能模块？（安装 / SSH / Doctor / 配置 / UI）
- 是否全量影响？还是特定条件下触发？

### Step 2: 收集证据

```bash
make artifacts    # 收集本地日志和 trace
```

检查以下日志源：
- **前端**: DevTools Console (Ctrl+Shift+I)
- **Rust**: 终端输出或 `~/.clawpal/logs/`
- **CI**: GitHub Actions 的 job log
- **Packaged app**: 系统日志目录（macOS: `~/Library/Logs/`, Linux: `~/.local/share/`）

### Step 3: 定位变更

```bash
git log --oneline -10                    # 最近提交
git bisect start HEAD <last_known_good>  # 二分定位
```

### Step 4: 决定回滚还是修复

| 条件 | 行动 |
|------|------|
| 影响面广 + 无快速修复 | 回滚 |
| 影响面窄 + 原因明确 | hotfix PR |
| 仅 CI 失败 + 不影响用户 | 正常修复 |

## 回滚流程

### 代码回滚

```bash
git revert <commit-sha>
git push origin develop
```

### 版本回滚

如果已发布的版本有问题：

1. 在 GitHub Releases 标记问题版本为 pre-release 或删除
2. 创建新的 RC 分支发布修复版本
3. 通知已安装用户（如有自动更新渠道）

### Doctor 自修复

对于已安装用户，ClawPal Doctor 可以：
- 检测配置损坏并修复
- 重装 OpenClaw 组件
- 回滚到上一个 snapshot

## 验证方法

回滚后执行：
```bash
make ci           # 本地 CI 全量检查
make build        # 确认构建通过
```

确认 GitHub Actions CI 全部通过。
