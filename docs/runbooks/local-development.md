# 本地开发启动

## 触发条件

首次 clone 仓库或切换分支后需要重新启动开发环境。

## 前置依赖

- Rust (stable)
- Node.js ≥ 18
- Bun (推荐) 或 npm

- 平台特定 Tauri 依赖（参考 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

## 启动步骤

1. 检查开发环境：
   ```bash
   make doctor
   ```

2. 安装前端依赖：
   ```bash
   make install
   ```

3. 启动开发模式（前端 + Tauri）：
   ```bash
   make dev
   ```

4. 仅启动前端（不含 Tauri）：
   ```bash
   make dev-frontend
   ```

## 验证与测试

```bash
make lint           # 全部 lint（TypeScript + Rust fmt + clippy）
make test-unit      # 全部单元测试（前端 + Rust）
make ci             # 本地完整 CI 检查
```

## 常见问题

### WebView 相关错误（Linux）

安装 `libwebkit2gtk-4.1-dev` 和相关依赖：

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev
```

### Rust 编译错误

```bash
rustup update stable
cargo clean
make build
```

### Rust 格式错误

```bash
make fmt            # 自动修复
```

### 前端类型错误

```bash
make typecheck
```

## 验证方法

应用窗口正常打开，首页渲染成功，DevTools 无报错。
