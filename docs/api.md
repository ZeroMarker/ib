# HTTP API 参考

所有响应都是 JSON（静态资源除外）。错误响应统一形状：

```json
{ "error": "面向用户的错误消息" }
```

金额和数量在 JSON 里**始终是字符串**（如 `"185.52"`、`"0"`、`"-12.5"`），
请求侧也必须传字符串。用裸数字会引入 `f64` 精度问题，服务端解析失败返回 400。

所有 `POST` 需要 `Content-Type: application/json`，否则 axum 返回 415。

## 状态码约定

| 状态码 | 含义 | 典型场景 |
| --- | --- | --- |
| 200 | 成功 | 查询、下单 |
| 201 | 已创建 | 注册、创建合约 |
| 204 | 成功但无内容 | 登出、撤单、注资、注入成交 |
| 400 | 请求不合法 | 邮箱格式、密码长度、金额非法、合约不存在 |
| 401 | 未认证 | 缺 Cookie、会话过期、账号密码错误 |
| 403 | 已认证但不允许 | 邮箱未验证且邮件服务已配置 |
| 409 | 状态冲突 | 邮箱已注册、订单不可撤、订单不可成交、合约重复 |
| 410 | 资源已失效 | 邮箱验证 token 过期或已用 |
| 415 | 不支持的媒体类型 | 缺 `Content-Type: application/json` |
| 500 | 服务端缺陷 | 消息恒为 `internal server error` |
| 503 | 依赖不可用 | 数据库打不开、`/api/health` 探活失败 |

映射由 `src/http.rs` 的 `ApiError` 统一定义。5xx 在返回前一定会写日志。

## 认证

### 会话 Cookie

| 项 | 值 |
| --- | --- |
| 名称 | `ib_session` |
| 属性 | `Path=/; HttpOnly; SameSite=Lax; Secure` |
| 有效期 | 30 天（Cookie `Max-Age` 与库内 `EXPIRES_AT` 一致） |
| 库内存储 | 只存 token 的 SHA-256，明文仅存在于 Cookie |

> **`Secure` 的本地影响**：Cookie 带 `Secure` 标记，通过 `http://127.0.0.1:8081` 访问时
> Chrome / Firefox 把 localhost 视为安全上下文，会正常保存；但通过局域网 IP 的明文
> HTTP 访问会被浏览器丢弃 Cookie，表现为「登录成功但一直未登录」。
> 本地开发请用 `localhost`，或直接走 HTTPS（Caddy）。

### `POST /api/auth/register`

```json
{ "email": "user@example.com", "password": "至少 8 位" }
```

`201 Created`，**不签发会话 Cookie**——必须先验证邮箱再登录。

```json
{ "user_id": "9eab5226-...", "email": "user@example.com", "email_verified": false }
```

| 情况 | 响应 |
| --- | --- |
| 成功 | 201，如上 |
| 邮箱格式非法 / 密码不在 8–128 字节 | 400 |
| 邮箱已注册 | 409 `email is already registered` |
| 邮件发送失败 | **仍返回 201**，账户保持 `email_verified: false`，失败写日志 |

密码长度按**字节**计（`(8..=128).contains(&password.len())`），多字节字符会占更多。

### `POST /api/auth/verify`

```json
{ "token": "邮件里的一次性 token" }
```

`200 OK`，返回 `email_verified: true` 的用户对象。

| 情况 | 响应 |
| --- | --- |
| 成功 | 200 |
| token 为空 | 400 |
| token 不存在 | 400 `invalid verification token` |
| token 过期 | 410 `verification token has expired` |

验证成功会**清除该用户全部未用 token**，不是只删当前这一个。
token 有效期 24 小时，库内只存 SHA-256。

前端在启动时读取 `?verify_token=` 查询参数自动调用本接口，并立刻从 URL 上抹掉。

### `POST /api/auth/resend-verification`

```json
{ "email": "user@example.com" }
```

`200 OK`，恒定返回 `{ "message": "ok" }`。

以下情况对调用方**完全不可区分**（防账户枚举）：邮箱不存在、已验证、邮件服务未配置、
Resend 调用失败。区别只体现在服务端日志。邮箱格式非法仍返回 400。

每个用户只保留一个未用 token：重发会作废之前发出的链接。

### `POST /api/auth/login`

```json
{ "email": "user@example.com", "password": "..." }
```

`200 OK` + `Set-Cookie`，返回用户对象。

| 情况 | 响应 |
| --- | --- |
| 成功 | 200 + Cookie |
| 邮箱格式非法 | 400 |
| 邮箱不存在或密码错误 | 401 `invalid email or password`（两者响应与耗时一致） |
| 邮箱未验证，且 `RESEND_API_KEY` 已配置 | 403 `email not verified; check your inbox for the verification email` |
| 邮箱未验证，但邮件服务未配置 | 200，**门禁降级放行**（防本地/开发环境把自己锁死） |

登录时会顺带清理过期的 `SESSIONS` 和 `EMAIL_VERIFICATIONS` 行。

### `POST /api/auth/logout`

无请求体。`204 No Content` + 清除 Cookie 的 `Set-Cookie`。
即使没有有效会话也返回 204（幂等）。

### `GET /api/auth/me`

`200 OK` 返回用户对象；无有效会话返回 401 `authentication required`。
前端用它做启动时的登录态恢复。

### `GET /api/health`

**无需认证**。执行真实的 `SELECT 1` 探活。

| 情况 | 响应 |
| --- | --- |
| 数据库可用 | 200 `{ "message": "ok" }` |
| 数据库不可用 | 503 `{ "error": "database unavailable" }` |

## 交易接口

以下所有接口都需要有效会话 Cookie（否则 401），并且只操作**调用者自己的**模拟账户
`SIM<user_id 前 13 位十六进制>`。跨用户访问由账户归属条件天然隔离。

模拟账户在首次访问时自动创建（`MARGIN` / `USD` / `ACTIVE`）。

### `GET /api/trading/overview`

UI 的主要读接口，一次返回全部账本快照。

```json
{
  "account":   { "account_id": "SIM9eab52263a104", "account_type": "MARGIN", "currency": "USD", "status": "ACTIVE" },
  "contracts": [ /* Contract */ ],
  "orders":    [ /* Order */ ],
  "positions": [ /* Position */ ],
  "cash":      [ /* CashBalance */ ],
  "fills":     [ /* Fill */ ]
}
```

注意 `contracts` 是**全站合约表**，不按账户过滤——合约是共享的静态参考数据。

### `GET /api/trading/contracts` · `POST /api/trading/contracts`

`GET` 返回全部合约数组。`POST` 新增合约，成功返回 `201` + 合约对象。

```json
{ "conid": 265598, "symbol": "AAPL", "sec_type": "STK", "exchange": "SMART", "currency": "USD" }
```

服务端校验：`conid > 0`、`symbol` 去空格后非空、`currency` 为 3 个 ASCII 字母。
`symbol`/`sec_type`/`exchange`/`currency` 均转大写。合约乘数固定 1.0，不可指定。

| 情况 | 响应 |
| --- | --- |
| 成功 | 201 |
| 校验失败 | 400 `invalid contract` |
| `conid` 或 `(symbol, sec_type, exchange, currency)` 重复 | 409 `contract already exists` |

### `GET /api/trading/orders` · `POST /api/trading/orders`

`GET` 返回本账户全部订单（不支持状态过滤，前端在客户端过滤）。

`POST` 请求体：

```json
{
  "conid": 265598,
  "side": "BUY",
  "order_type": "LMT",
  "quantity": "100",
  "lmt_price": "185.50",
  "aux_price": null
}
```

服务端**自行分配 `order_id`**（按账户 `MAX(ORDER_ID) + 1`，与 CLI 不同），
成功返回 `200`（注意不是 201）+ 订单对象，状态 `Submitted`。

校验顺序很重要：先校验参数和合约存在性，**再**分配订单号，避免无效请求消耗 ID。

| 情况 | 响应 |
| --- | --- |
| 成功 | 200 + Order |
| 数量/价格/类型/方向非法 | 400 |
| `conid` 不存在 | 400 `contract not found` |
| 数值超过 6 位小数 | 400 |

`LMT` / `STP_LMT` 必须给 `lmt_price`，`STP` / `STP_LMT` 必须给 `aux_price`。
`scaled` 对超过 6 位小数的输入直接拒绝而非四舍五入。

### `POST /api/trading/orders/{order_id}/cancel`

无请求体。`204 No Content`。

`409 order is not cancellable`：订单不存在、不属于本账户，或已处于 `Filled` / `Cancelled`。
判断在 SQL 的 `WHERE` 里完成，不需要前置扫描。

### `POST /api/trading/orders/{order_id}/fill`

```json
{ "price": "185.52", "exec_id": "可选" }
```

把订单剩余数量一次性全部成交，`204 No Content`。

`exec_id` 省略时自动生成 `WEB` + 21 位十六进制（合计 24 字符，正好是上限）。
需要重放同一笔成交时显式传回同一个 `exec_id`——这是幂等的。

| 情况 | 响应 |
| --- | --- |
| 成功 | 204 |
| `price` 不是合法数值 / ≤ 0 | 400 |
| 订单不存在、非本账户、已成交/已撤、剩余为 0 | 409 |

### `GET /api/trading/positions`

返回本账户持仓数组。`position` 带符号：正数多头、负数空头。`avg_cost` 在无持仓时为 `null`。

### `GET /api/trading/cash` · `POST /api/trading/cash`

`GET` 返回本账户各币种余额数组。

`POST` 请求体，**直接覆盖**而非累加：

```json
{ "currency": "USD", "amount": "100000" }
```

`204 No Content`。校验规则：

| 情况 | 响应 |
| --- | --- |
| 成功 | 204 |
| `currency` 不是 3 个 ASCII 字母 | 400 |
| `amount` ≤ 0（含 0） | 400 `cash amount must be positive` |
| `amount` 超过 6 位小数 | 400 |

> **正数校验在服务端**。前端 `useTrading.setCash` 也会拦一次，但那是体验优化，
> 不是安全边界——直接调 API 现在同样拿不到负余额。
> CLI 的 `ib cash set` 刻意保留写入负数的能力（模拟透支），
> 两者行为不同是有意的。

### `GET /api/trading/fills`

返回本账户成交记录，按 `EXEC_TIME` 倒序（最新在前）。

## 静态资源

由同一个 Rust 服务提供（`src/web.rs`），内容通过 `include_str!` / `include_bytes!`
编译进二进制。

| 路径 | Content-Type | 用途 |
| --- | --- | --- |
| `/`、`/login`、`/register` | `text/html` | 应用外壳（`dist/index.html`） |
| `/assets/app.js` | `application/javascript` | 前端包 |
| `/assets/index.css` | `text/css` | 样式 |
| `/manifest.webmanifest` | `application/manifest+json` | PWA manifest |
| `/sw.js` | `application/javascript` | Service Worker（须在根路径以控制整个 scope） |
| `/icons/icon-192.png` | `image/png` | PWA 图标 |
| `/icons/icon-512.png` | `image/png` | PWA 图标 |
| `/icons/icon.svg` | `image/svg+xml` | 矢量图标 |

**所有静态资源都带 `Cache-Control: no-cache, no-store, must-revalidate`。**
内容虽是编译期常量，但用于击穿 Service Worker 缓存的 `?v=` 查询串会随构建变化，
陈旧缓存会把用户钉在旧版本前端上。

前端使用 **hash 路由**（`#overview` / `#trade` / `#orders` / `#positions` / `#fills`），
因此 `/login`、`/register` 两个路由实际上不参与前端导航，保留它们只是为了让直接
输入这些路径的旧书签仍能打开应用外壳。

## 完整示例

```bash
# 注册（无 Cookie）
curl -si -X POST localhost:8081/api/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"user@example.com","password":"correct horse"}' | head -1

# 未验证时登录被拒
curl -si -X POST localhost:8081/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"user@example.com","password":"correct horse"}' | head -1
# HTTP/1.1 403 Forbidden

# 验证后登录，带 Cookie 存会话
curl -s -c jar.txt -X POST localhost:8081/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"user@example.com","password":"correct horse"}'

# 用 Cookie 下单
curl -s -b jar.txt -X POST localhost:8081/api/trading/orders \
  -H 'Content-Type: application/json' \
  -d '{"conid":265598,"side":"BUY","order_type":"LMT","quantity":"100","lmt_price":"185.50","aux_price":null}'

# 注入成交并查账本
curl -s -b jar.txt -X POST localhost:8081/api/trading/orders/1/fill \
  -H 'Content-Type: application/json' -d '{"price":"185.52","exec_id":"EX-20260930-0001"}'
curl -s -b jar.txt localhost:8081/api/trading/overview
```
