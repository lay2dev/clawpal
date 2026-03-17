# ClawPal 量化指标体系

本文档定义 ClawPal 项目的量化指标、当前基线、目标值和量化方式。

指标分为三类：
1. **工程健康度** — PR、CI、测试、文档（来自 Harness Engineering 基线文档）
2. **运行时性能** — 启动、内存、command 耗时、包体积
3. **Tauri 专项** — command 漂移、打包验证、全平台构建

## 1. 工程健康度

### 1.1 Commit / PR 质量

| 指标 | 基线值 (2026-03-17) | 目标 | 量化方式 | CI Gate |
|------|---------------------|------|----------|---------|
| 单 commit 变更行数 | 未追踪 | ≤ 500 行 | `git diff --stat` | ✅ |
| PR 中位生命周期 | 1.0h | ≤ 4h | GitHub API | — |

### 1.2 CI 稳定性

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| CI 成功率 | 75% | ≥ 90% | workflow run 统计 | — |
| CI 失败中环境问题占比 | 未追踪 | 趋势下降 | 手动分类 | — |

### 1.3 测试覆盖率

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| 行覆盖率 (core + cli) | 74.4% | ≥ 80% | `cargo llvm-cov` | ✅ 不得下降 |
| 函数覆盖率 | 68.9% | ≥ 75% | `cargo llvm-cov` | ✅ 不得下降 |

### 1.4 代码可读性

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| commands/mod.rs 行数 | 8,842 | ≤ 2,000 | `wc -l` | — |
| App.tsx 行数 | 1,787 | ≤ 500 | `wc -l` | — |
| 单文件 > 500 行数量 | 未统计 | 趋势下降 | 脚本统计 | — |

## 2. 运行时性能

### 2.1 启动与加载

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| 冷启动到首屏渲染 | 待埋点 | ≤ 2s | `performance.now()` 差值 | ✅ |
| 首个 command 响应时间 | 待埋点 | ≤ 500ms | 首次 invoke 到返回的耗时 | ✅ |
| 页面路由切换时间 | 待埋点 | ≤ 200ms | React Suspense fallback 持续时间 | — |

**埋点方案**:

前端（`src/App.tsx`）:
```typescript
// 在模块顶部记录启动时间
const APP_START = performance.now();

// 在 App() 首次渲染完成的 useEffect 中
useEffect(() => {
  const ttfr = performance.now() - APP_START;
  console.log(`[perf] time-to-first-render: ${ttfr.toFixed(0)}ms`);
  invoke("log_app_event", {
    event: "perf_ttfr",
    data: JSON.stringify({ ttfr_ms: Math.round(ttfr) })
  });
}, []);
```

### 2.2 内存

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| 空闲内存占用（Rust 进程） | 待埋点 | ≤ 80MB | `sysinfo` crate 或 OS API | ✅ |
| 空闲内存占用（WebView） | 待埋点 | ≤ 120MB | `performance.memory` (Chromium) | — |
| SSH 长连接内存增长 | 待埋点 | ≤ 5MB/h | 连接后定期采样 | — |

**埋点方案**:

Rust 侧（`src-tauri/src/commands/overview.rs` 或新建 `perf.rs`）:
```rust
#[tauri::command]
pub fn get_process_metrics() -> Result<ProcessMetrics, String> {
    let pid = std::process::id();
    // 读取 /proc/{pid}/status (Linux) 或 mach_task_info (macOS)
    // 返回 RSS, VmSize 等
}
```

### 2.3 构建产物

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| macOS ARM64 包体积 | 12.6 MB | ≤ 15 MB | CI build artifact | ✅ |
| macOS x64 包体积 | 13.3 MB | ≤ 15 MB | CI build artifact | ✅ |
| Windows x64 包体积 | 16.3 MB | ≤ 20 MB | CI build artifact | ✅ |
| Linux x64 包体积 | 103.8 MB | ≤ 110 MB | CI build artifact | ✅ |
| 前端 JS bundle 大小 (gzip) | 待统计 | ≤ 500 KB | `vite build` + `gzip -k` | ✅ |

**CI Gate 方案**:

在 `ci.yml` 的 frontend job 中添加:
```yaml
- name: Check bundle size
  run: |
    bun run build
    BUNDLE_SIZE=$(du -sb dist/assets/*.js | awk '{sum+=$1} END {print sum}')
    BUNDLE_KB=$((BUNDLE_SIZE / 1024))
    echo "Bundle size: ${BUNDLE_KB}KB"
    if [ "$BUNDLE_KB" -gt 512 ]; then
      echo "::error::Bundle size ${BUNDLE_KB}KB exceeds 512KB limit"
      exit 1
    fi
```

在 `pr-build.yml` 中添加包体积检查:
```yaml
- name: Check artifact size
  run: |
    # 平台对应的限制值 (bytes)
    case "${{ matrix.platform }}" in
      macos-latest)   LIMIT=$((15 * 1024 * 1024)) ;;
      windows-latest) LIMIT=$((20 * 1024 * 1024)) ;;
      ubuntu-latest)  LIMIT=$((110 * 1024 * 1024)) ;;
    esac
    ARTIFACT_SIZE=$(du -sb target/release/bundle/ | awk '{print $1}')
    if [ "$ARTIFACT_SIZE" -gt "$LIMIT" ]; then
      echo "::error::Artifact size exceeds limit"
      exit 1
    fi
```

### 2.4 Command 性能

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| 本地 command P95 耗时 | 待埋点 | ≤ 100ms | Rust `Instant::now()` | ✅ |
| SSH command P95 耗时 | 待埋点 | ≤ 2s | 含网络 RTT | — |
| Doctor 全量诊断耗时 | 待埋点 | ≤ 5s | 端到端计时 | — |
| 配置文件读写耗时 | 待埋点 | ≤ 50ms | `Instant::now()` | — |

**埋点方案**:

在 command 层添加统一计时 wrapper（`src-tauri/src/commands/mod.rs`）:
```rust
use std::time::Instant;
use tracing::{info, warn};

/// 记录 command 执行耗时，超过阈值发出 warning
pub fn trace_command<F, T>(name: &str, threshold_ms: u64, f: F) -> T
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    let ms = elapsed.as_millis() as u64;
    if ms > threshold_ms {
        warn!(command = name, elapsed_ms = ms, "command exceeded threshold");
    } else {
        info!(command = name, elapsed_ms = ms, "command completed");
    }
    result
}
```

## 3. Tauri 专项

| 指标 | 基线值 | 目标 | 量化方式 | CI Gate |
|------|--------|------|----------|---------|
| Command 前后端漂移次数 | 未追踪 | 0 | contract test | ✅ (Phase 3 延后项) |
| Packaged app smoke 通过率 | 无 smoke test | 100% | packaged smoke CI | ✅ (Phase 3 延后项) |
| 全平台构建通过率 | 100% | ≥ 95% | PR build matrix | ✅ |

## 4. CI Gate 实施计划

### 阶段 1: 立即可加（本 PR 后续 commit）

1. **单 commit 变更行数 gate** — PR 中每个 commit 不超过 500 行（additions + deletions）
2. **前端 bundle 大小 gate** — `ci.yml` frontend job 增加 `du` 检查
3. **覆盖率不得下降 gate** — 已有 `coverage.yml`，确认 delta ≥ 0 时 fail

**Commit 大小检查脚本**（加入 `ci.yml`）:
```yaml
- name: Check commit sizes
  run: |
    MAX_LINES=500
    BASE="${{ github.event.pull_request.base.sha }}"
    HEAD="${{ github.sha }}"
    FAIL=0
    for COMMIT in $(git rev-list $BASE..$HEAD); do
      SHORT=$(git rev-parse --short $COMMIT)
      SUBJECT=$(git log --format=%s -1 $COMMIT)
      STAT=$(git diff --shortstat ${COMMIT}^..${COMMIT} 2>/dev/null || echo "0")
      ADDS=$(echo "$STAT" | grep -oP '\d+ insertion' | grep -oP '\d+' || echo 0)
      DELS=$(echo "$STAT" | grep -oP '\d+ deletion' | grep -oP '\d+' || echo 0)
      TOTAL=$((${ADDS:-0} + ${DELS:-0}))
      echo "$SHORT ($TOTAL lines): $SUBJECT"
      if [ "$TOTAL" -gt "$MAX_LINES" ]; then
        echo "::error::Commit $SHORT exceeds $MAX_LINES line limit ($TOTAL lines): $SUBJECT"
        FAIL=1
      fi
    done
    if [ "$FAIL" -eq 1 ]; then
      echo "::error::One or more commits exceed the $MAX_LINES line limit. Split into smaller commits."
      exit 1
    fi
```

### 阶段 2: 埋点后可加

3. **冷启动时间 gate** — 前端埋点 + E2E 测试中采集
4. **command 耗时 gate** — Rust wrapper + 单元测试中断言
5. **内存占用 gate** — `get_process_metrics` command + E2E 测试中采集

### 阶段 3: 基础设施完善后

6. **包体积 gate** — `pr-build.yml` 中按平台检查
7. **Packaged app smoke gate** — 需要 headless 桌面环境或 Xvfb

## 5. 指标记录与趋势

每周熵治理时记录到 `docs/runbooks/entropy-governance.md` 的指标表中。

建议每月输出一次指标趋势报告，重点关注：
- 覆盖率是否稳步上升
- PR 粒度是否持续减小
- CI 成功率是否稳定在 90% 以上
- 包体积是否异常增长
- 新增 command 是否有对应的 contract test
