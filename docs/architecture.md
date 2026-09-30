# 架构

`ib` 是一个单进程、单数据库文件的模拟交易平台。Rust 既是 HTTP 服务也是 CLI，
前端构建产物通过 `include_str!` 嵌入二进制，因此运行时没有任何外部服务依赖，
唯一的外部依赖是可选的 Resend（仅用于邮箱验证邮件）。

## 进程与依赖拓扑

```
                      ┌──────────────────────────────┐
   浏览器  ──HTTPS──▶  │  Caddy (可选)               │
                      │  127.0.0.1:8081 反向代理     │
                      └──────────────┬───────────────┘
                                     │  HTTP
                      ┌──────────────▼───────────────┐
                      │  ib (单个 Rust 进程)         │
                      │  ├─ axum Router              │
                      │  ├─ tokio 运行时             │
                      │  └─ 嵌入的 frontend/dist     │
                      └───────┬──────────────┬───────┘
                              │              │ 仅在配置 Resend 时
                       rusqlite (bundled)   └──HTTPS──▶ api.resend.com
                              │
                    ib.sqlite3 (WAL)
```

- SQLite 以 `bundled` 特性静态编译进二进制，不需要系统数据库客户端。
- Resend 是**可选**依赖：未配置 `RESEND_API_KEY` 时平台完全可用，只是邮箱验证被跳过。

## 构建顺序约束

`src/web.rs` 用 `include_str!` / `include_bytes!` 嵌入 `frontend/dist/**`。
Rust 编译器在编译期就要求这些文件存在，因此：

```
frontend (npm run build)  →  frontend/dist  →  cargo build  →  ib 二进制
```

反过来不成立：先 `cargo build` 会因为 `frontend/dist` 缺失而失败。
CI（`.github/workflows/ci.yml`）和 [部署文档](deployment.md) 都按这个顺序编排。

`frontend/dist` 已提交到仓库，所以只改后端时可以跳过前端构建；只改前端时必须重建前端
再重建后端，否则改动不会进入二进制。

## 后端模块地图

模块边界由 `src/lib.rs` 的文档注释声明，抽成库目标是让 `tests/api.rs` 能驱动**真实
router** 而不是复制一份接线。

| 模块 | 职责 | 不做什么 |
| --- | --- | --- |
| `db` | 迁移执行、全部 SQL、`Pool`、定点换算、领域写入逻辑 | 不认识 HTTP |
| `models` | 领域结构体、`Decimal` 的 serde 序列化 | 不含行为 |
| `auth` | 注册/登录/登出/会话/邮箱验证、`AppState`、`shutdown_signal` | 不写交易账本 |
| `trading` | `/api/trading/*` 处理器与请求结构体 | 不直接写 SQL，一律走 `db` |
| `http` | `ApiError`、错误响应渲染、`run_db` 阻塞桥 | 不知道具体端点 |
| `web` | 8 个静态资源处理器（内容类型 + `no-store`） | 无业务逻辑 |
| `email` | Resend HTTP 适配器 | 不决定何时发信（由 `auth` 决定） |
| `lib` | `app_router()` 路由表 + `serve()` 生命周期 | — |
| `main` | CLI 参数解析与命令分发 | 无业务逻辑 |

### 错误分层

数据层函数返回 `Result<_, String>` 或 `Result<_, rusqlite::Error>`，**不携带 HTTP 状态码**。
只有 handler 决定状态码，通过 `src/http.rs` 的 `ApiError` 表达：

```rust
enum ApiError {
    Invalid(String),           // 400，消息面向用户、可操作
    Unauthenticated(&'static str), // 401
    Forbidden(&'static str),   // 403
    Gone(&'static str),        // 410，token 已不可用
    Conflict(&'static str),    // 409，状态冲突
    Unavailable(&'static str), // 503，依赖不可用
    Internal(&'static str),   // 500，消息必须泛化
}
```

这层间接是为了让 handler 能用 `?`，也让数据层签名不必出现 `Response`。
`IntoResponse` 在渲染前对 5xx 打日志（`log_if_server_error`），5xx 一律意味着服务端缺陷。

## 请求生命周期

每个 HTTP 请求的固定路径：

```
axum handler (async)
  └─ run_db(state, closure)                        src/http.rs
       └─ tokio::task::spawn_blocking               不占用 reactor worker
            └─ closure(&Pool)
                 ├─ pool.get() → MutexGuard<Connection>   ← 串行化点
                 ├─ auth::current_user(&conn, headers)
                 └─ db::* 函数（全部 SQL）
```

三条硬性后果：

1. **SQLite 查询和 Argon2 哈希都离开 Tokio 工作线程。** Argon2 默认参数耗时数十毫秒，
   在 worker 上直接算会拖垮整个运行时。
2. **并发被单把锁串行化。** `Pool` 是 `Mutex<Connection>` 而非连接池——SQLite 同一时刻
   只有一个写者，多连接只会带来 `SQLITE_BUSY` 而不是吞吐。
3. **锁持有到响应序列化完成。** `Json(...).into_response()` 在闭包内部就地序列化，
   所以大 payload 的编码时间也在锁内。这是当前架构下的已知取舍：账本规模小，
   换来的是无死锁、无连接池配置、无写冲突。

## 关键设计决策

### 定点整数微单位与浮点的区别

金额和数量在数据库里是 `INTEGER`，含义为 `1_000_000` = 1 单位（`DECIMAL_SCALE`）。
应用层用 `rust_decimal::Decimal`，读写时缩放：

- **输入** 走 `scaled()`：小数位超过 6 位直接**拒绝**（`1.0000001` 报错）。
- **派生值** 走 `scaled_round()`：先 `round_dp(6)` 再缩放。这用于平均成本混合和现金差额，
  它们是算术结果而非用户输入，静默四舍五入比报错更合理。

任何环节都不经过 `f64`，因此 `185.52` 存进去读出来必然是 `185.52`。

JSON 侧由 `models.rs` 的 `serialize_decimal` 统一 `normalize()` 去尾零，输出规范化字符串
（`"185.52"`、`"0"`、`"-12.5"`），前端不会看到 `"185.520000"`，也不必处理精度丢失。

### 迁移账本

`migrations/*.sql` 通过 `include_str!` 编译进二进制，按文件名顺序执行，已应用的记在
`SCHEMA_MIGRATIONS` 表（`NAME` 主键 + `APPLIED_AT`）里跳过。

三个易错点都处理了：

- **记录时机**：迁移语句全部成功后才写账本。失败的迁移保持 pending，下次运行会重试，
  不会被误标为已应用。
- **SQL 切分**：`split_statements()` 跟踪单引号/双引号跨度，引号外的 `--` 注释行整行丢弃。
- **触发器体**：`BEGIN ... END` 内部的分号不能切分。切分器维护块深度，
  只在深度 0 处切分；`END` 要求独占一行，以免把 `CASE ... END` 误判成块结束。

001 的每条语句都是 `CREATE ... IF NOT EXISTS`，让迁移账本出现之前创建的存量库
（部署实例的真实状态）也能安全重复执行 `init-db`。

### 模拟账户与用户绑定

`user_account_id(user_id)` 从 UUID 里取前 13 个十六进制字符，拼成 `SIM<13 hex>`（16 字符）。
纯函数、确定性、无随机量，所以同一用户永远映射到同一模拟账户；`ensure_user_account`
用 `INSERT OR IGNORE` 处理并发首次登录。

### 成交幂等

`record_fill` 在一个 `BEGIN IMMEDIATE` 事务里联动订单、持仓、现金：

1. 同 `EXEC_ID` + 同订单 + 同账户 + 同价格 → 直接 `Ok(())`（重试是 no-op）。
2. `EXEC_ID` 已被别的成交占用 → 报错。
3. 订单必须处于 `Submitted` / `PreSubmitted`，否则报错。

因此策略重放可以安全复用 `EXEC_ID`，不会重复记账。

### 前后端类型契约

`api-schema.json` 是共享的线上契约。`src/models.rs` 的单元测试断言序列化字段与之一致，
`frontend/src/types.ts` 在编译期断言键集一致。两端指向同一个文件，所以服务端加字段
而前端漏改会在 `tsc` 阶段失败，而不是运行时拿到 `undefined`。

值类型在 `types.ts` 里手写：TypeScript 会把导入 JSON 的字符串值放宽成 `string`，
无法从 `"string|null"` 推导联合类型。可编译期断言的只有键名。

### 优雅关停

`axum::serve(...).with_graceful_shutdown(auth::shutdown_signal())` 监听 SIGINT 与 SIGTERM。
没有它的话 `systemctl stop` 会直接关监听器，丢弃正在 `spawn_blocking` 里执行到一半的
事务性账本写入。现在是先排空连接再退出。

## 前端结构

前端是 Vite + React + TypeScript，无路由库、无状态管理库，视图切换用
`location.hash`。结构、状态约定、快捷键和 PWA 细节见[前端文档](frontend.md)。

```
main.tsx              挂载点
└─ App.tsx            启动：注册 SW、捕获安装提示、恢复登录态
   ├─ Auth            未登录：登录/注册/验证横幅
   └─ Dashboard       已登录：唯一调用 useTrading 的地方
      ├─ Layout.tsx   Sidebar/Header/MobileNav/VerifyNote/Alerts/
      │               WorkspaceNav/FillModal/Footer
      ├─ OverviewPage / TradePage / OrdersPage / PositionsPage / FillsPage
      └─ components/index.tsx   PanelTitle/DataTable/Empty/Metric 通用展示件
```

与后端耦合的两点值得在这里记一笔：

- **账本只有一个数据源。** 前端不维护第二份状态，全部派生自
  `GET /api/trading/overview` 的单次返回，因此不存在前后端状态不一致的问题。
- **PWA 明确不缓存 `/api/`。** 交易数据永不过期；离线时只能看到页面壳。
  静态资源的 `no-store` 策略是配套的：内容虽是编译期常量，但 `?v=` 版本串
  会随构建变化，陈旧缓存会把用户钉在旧版本上。

## 安全姿态

| 面向 | 做法 |
| --- | --- |
| 密码 | Argon2 哈希 |
| 会话 | 明文 token 只在 Cookie，库里只存 SHA-256 |
| Cookie | `HttpOnly; Secure; SameSite=Lax; Path=/`，30 天 |
| 邮箱枚举 | 未知邮箱也执行一次 Argon2 校验以对齐耗时；`resend-verification` 对所有情况返回同一个 `ok` |
| 越权 | 交易接口一律经 `auth::current_user` + `ensure_user_account` 限定到调用者自己的模拟账户 |
| 限流 | 无。见 TODO「后续增强」 |
| CSRF | 依赖 `SameSite=Lax` + 仅接受 `application/json`，无 token |
