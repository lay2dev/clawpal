# 本地开发启动

## 触发条件

首次 clone 仓库或切换分支后需要重新启动开发环境。

## 前置依赖

- Rust (stable)
- Node.js ≥ 18
- Bun (推荐) 或 npm
- 平台特定 Tauri 依赖（参考 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

## 启动步骤

1. 安装前端依赖：
   ```bash
   bun install
   ```

2. 启动开发模式：
   ```bash
   bun run dev:tauri
   ```

3. 仅启动前端（不含 Tauri）：
   ```bash
   bun run dev
   ```

## 常见问题

### WebView 相关错误（Linux）

安装 `libwebkit2gtk-4.1-dev` 和相关依赖。

### Rust 编译错误

```bash
rustup update stable
cargo clean
```

### 前端类型错误

```bash
bun run typecheck
```

## 验证方法

应用窗口正常打开，首页渲染成功，DevTools 无报错。
