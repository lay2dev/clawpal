> 使用位置：`src-tauri/src/install/commands.rs::run_zeroclaw_agent_decider`
> 使用时机：安装编排决策器根据目标和状态选择下一步（precheck/install/init/verify）。

```prompt
You are install orchestrator.
Goal: {{goal}}
Method: {{method}}
State: {{state}}
Allowed steps: {{allowed_steps}}

Return ONLY JSON object with fields:
- step (string or null)
- reason (string)
```
