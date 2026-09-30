# 项目待办与执行记录

更新时间：2026-09-30

## 当前状态

模拟交易账本、用户认证、终端式前端、Caddy/systemd 部署均已可用。质量门禁
（CI、`fmt`/`clippy`、Prettier/ESLint/Vitest）与 98 个测试就位。
平台目前是**闭环的手动注入式纸上交易**：没有行情源、没有撮合规则、
没有手续费与风控。

详细设计见 [docs/](README.md#文档)，本文件只跟踪待办与执行历史。

## 待办

### P0：待确认

- [ ] **确认线上库里没有固定密码的测试用户**。开发期曾「预留固定密码的测试用户」，
      若仍存在于生产库，应删除或改用随机密码。这需要能访问生产数据库才能核实。

### P1：后端能力

- [ ] **行情驱动撮合**。需要行情源或回测时钟；当前成交价全靠 `fill add` 注入。
- [ ] **手续费、保证金和风控**。需要先明确模拟规则（费率模型、保证金率、
      强制平仓条件）再实现。
- [ ] **账户资产净值、未实现盈亏和行情快照**。`POSITIONS.MARKET_PRICE` 列已预留但未使用。
- [ ] **订单服务端分页与筛选**。现在分页和状态筛选都在前端做
      （`GET /api/trading/orders` 不接受过滤参数），订单量增长后会成为瓶颈。
- [ ] **限流**。注册、登录、重发验证邮件均无限流，是当前最容易被滥用的入口。

### P2：前端与产品

- [ ] **持仓市值、未实现盈亏、行情时间戳展示**（需 P1 的行情能力）。
- [ ] **手续费、保证金和风控状态展示**（需 P1 的风控能力）。
- [ ] **行情自选列表和价格卡片**（需 P1 的行情能力）。
- [ ] **多币种资产汇总和账户设置**。`POSITIONS` 主键是 `(ACCOUNT_ID, CONID)`，
      没有币种维度，同合约的多币种持仓无法表达。
- [ ] **订单成交明细与审计时间线**。`PARENT_ORDER_ID` 列已预留但未使用。
- [ ] **管理员与普通用户权限分离**。
- [ ] **多模拟账户切换和资金划转**。当前每个用户固定绑定一个 `SIM*` 账户。
- [ ] **策略运行监控和回测结果页面**。
- [ ] **深色模式、浏览器通知**。（键盘快捷键已完成，见 P3。）
- [ ] **React 错误边界**。渲染期异常目前会导致整页白屏。

### P3：工程质量

- [ ] **覆盖率报告与属性测试**。定点换算和 SQL 切分器很适合 property-based testing。
- [ ] **多请求并发测试**。`Pool` 串行化了一切，顺序执行已足够；引入真实并发后才需要竞态测试。

### 已知取舍（暂不修，但要知道）

- [ ] **会话 Cookie 带 `Secure`**，明文 HTTP 下只有 `localhost` 会保留 Cookie。
      局域网 IP 访问会表现为「登录成功但一直未登录」。
- [ ] **CSRF 依赖 `SameSite=Lax` + 仅接受 `application/json`**，没有 token。
- [ ] **单连接串行化**。所有请求在 `Mutex<Connection>` 上排队，锁持有到响应序列化完成。
      账本规模小的时候这是划算的取舍。
- [ ] **`api-schema.json` 的值类型不参与编译期检查**。TypeScript 会把导入的 JSON
      字符串放宽成 `string`，`"string|null"` 推不出联合类型，所以只有键集被断言。
      值写错（比如把 `lmt_price` 声明成 `string` 而非 `string | null`）
      只有运行时或人工 review 能发现。
- [ ] **`Overview` 契约没有 Rust 侧单元测试**。它定义在 `src/trading.rs` 且字段私有，
      目前只由前端断言和读真实端点的集成测试间接覆盖。

## 已完成能力

### 模拟交易账本

- [x] 账户、合约、订单、成交、持仓、现金六张表，含完整外键与 CHECK 约束
- [x] MKT/LMT/STP/STP_LMT × BUY/SELL，含限价单必填限价、止损单必填触发价
- [x] 订单号按账户 `MAX+1` 分配；下单前先校验参数再分配单号
- [x] 撤单：SQL `WHERE` 内判定可撤状态，不做前置全表扫描
- [x] 注入成交：单事务联动订单、持仓、现金
- [x] `EXEC_ID` 幂等：同 ID 重试是 no-op，跨成交复用被拒
- [x] 持仓平均成本：平仓清零、同向混合、反手取成交价、部分减仓保留原成本
- [x] 定点整数微单位（`1_000_000` = 1），输入超 6 位小数报错，派生值舍入

### 认证与邮箱验证

- [x] 邮箱注册（归一化 + 唯一索引），Argon2 密码哈希
- [x] 登录、登出、会话恢复；30 天 HttpOnly 会话 Cookie，库内只存 token 哈希
- [x] Resend 邮箱验证：注册发信、验证链接与验证码、24 小时有效、一次即焚
- [x] 未配置 Resend 时登录门禁自动降级（防本地环境锁死），`email_verified` 恒为 `false`
- [x] 防账户枚举：未知邮箱执行等时 Argon2；重发验证对所有情况返回同一个 `ok`
- [x] 登录时清理过期 `SESSIONS` / `EMAIL_VERIFICATIONS` 行
- [x] 生产发信域名 `20070809.xyz` 已验证并上线

### 前端

- [x] Vite + React + TypeScript，5 个 hash 路由视图（总览/交易/订单/持仓/成交）
- [x] 终端式侧栏导航、会话快照、系统状态、模拟环境提示
- [x] 订单状态筛选、分页（每页 8）、详情展开、撤单、注资成交
- [x] 合约搜索、重复选择拦截、表单校验、提交中状态与防重复提交
- [x] 键盘快捷键：`1`–`5` 切换视图、`R` 刷新、`Esc` 关闭弹窗
- [x] 标签页标题随视图同步
- [x] 邮箱验证三路径：URL 自动验证、横幅粘贴验证码、登录 403 后重发
- [x] PWA Manifest、192/512 图标、Service Worker、移动端安全区域适配
- [x] 离线策略：只缓存页面壳，交易 API 显式不进缓存

### 工程质量

- [x] CI（`.github/workflows/ci.yml`）：后端 `fmt`/`clippy -D warnings`/测试，
      前端 `format:check`/`lint`/`test`/`build`
- [x] 抽出 `src/lib.rs` 库目标 + 瘦 `main.rs`，`tests/` 驱动真实 router
- [x] 编译期约束：`unsafe_code = "forbid"`、clippy `all` 提为 warn
- [x] `ApiError` 小枚举取代 `Box<Response>` 穿过数据层
- [x] 同步数据库调用与 Argon2 哈希全部迁移到 `spawn_blocking`
- [x] 优雅关停：SIGINT/SIGTERM 先排空连接
- [x] 迁移账本 `SCHEMA_MIGRATIONS` + 幂等 DDL + 引号感知 SQL 切分
- [x] 过期行清理 `purge_expired` + 配套索引（迁移 004）
- [x] Resend 客户端 `LazyLock` 单例 + 5s/10s 超时
- [x] **入金金额正数校验下沉到服务端**。`POST /api/trading/cash` 现在拒绝 ≤0，
      不再依赖前端 `useTrading.setCash` 的检查。
- [x] **CLI 友好错误**。数据层错误打印 `ib: <操作> failed: <原因>` 并以退出码 `1`
      结束，不再 panic（此前重复账户、FK 违规都输出 Rust backtrace + 退出码 101）。
      `add_account` / `set_position` 相应改为返回 `Result`。
- [x] **Service Worker 版本串收敛到 `frontend/version.json`**。原先 `CACHE_NAME` /
      `ASSET_VERSION` / `assetVersion` / SW 注册 URL 四处需人工同步，现在单一来源：
      Vite 插件在 `closeBundle` 注入 `sw.js` 的占位符，前端从同一 JSON 读取
      `?v=`。占位符缺失时构建直接失败，不会把 `?v=__ASSET_VERSION__` 发出。
- [x] **Vite 开发代理**。`server.proxy` 把 `/api` 转发到 `ib serve`，
      `npm run dev` 可直接联调（可用 `IB_API_TARGET` 改目标）。
- [x] **迁移切分器支持 `BEGIN ... END` 触发器**。`split_statements()` 现在跟踪
      块深度，只在深度 0 处切分；`END` 要求独占一行以免误判 `CASE ... END`。
- [x] **前后端类型契约**。`api-schema.json` 为单一来源：`src/models.rs` 单元测试
      断言序列化字段与之一致，`types.ts` 在编译期断言键集一致。
      类型全部补齐（此前 `Position`/`Cash`/`Fill` 是服务端字段的子集）。
- [x] 98 个测试：23 单元 + 20 端到端 + 50 前端 + 5 契约

### 部署

- [x] Caddy 反向代理（HTTPS + gzip）
- [x] systemd 单元：非 root、`StateDirectory`、`ProtectHome=read-only` 沙箱
- [x] `ib init-auth`：给已有交易库补认证表，不碰账本
- [x] 升级流程与备份注意事项

## 执行记录

### 2026-09-29 质量加固与重构

质量门禁：

- [x] 新增 CI（`.github/workflows/ci.yml`）：后端 `fmt`/`clippy -D warnings`/测试，
      前端 `format:check`/`lint`/`test`/`build`；后端先构建前端产物
      （`include_str!` 依赖 `frontend/dist`）。
- [x] 前端接入 Prettier + ESLint 10（flat config、`typescript-eslint`、React Hooks、
      Vitest 5 + Testing Library），`npm run check` 一键跑格式/静态/测试。
- [x] 清理依赖漏洞：`npm audit` 0 vulnerabilities。

真实缺陷修复：

- [x] `email.rs` 每次发信都新建 `reqwest::Client` 且无超时：改为 `LazyLock` 单例共享
      连接池，并加 5s 连接 / 10s 请求超时，避免 Resend 挂起时请求永久阻塞。
- [x] 服务无优雅关停：`systemctl stop` 会直接丢弃进行中的账本写入。
      `axum::serve` 改为 `with_graceful_shutdown`，监听 SIGINT/SIGTERM 先排空连接再退出。
- [x] `SESSIONS` / `EMAIL_VERIFICATIONS` 过期行只过滤不删除，表格无限增长：
      新增 `004_maintenance_indexes.sql` 与登录时的 `purge_expired` 清理。
- [x] 迁移按 `;` 朴素切分，字符串字面量或触发器体内的分号会破坏 SQL：
      改为跟踪引号跨度的切分器。
- [x] `resend-verification` 在未配置 Resend 时对已知邮箱返回 503、未知邮箱返回 200，
      构成账户枚举预言机：统一为通用 `ok`，失败只记日志。
- [x] clippy `result_large_err`：`Box<Response>` 被穿过数据层传递，改为小枚举 `ApiError`。

结构重构：

- [x] 迁移版本表 `SCHEMA_MIGRATIONS`：`init-db` 幂等，已应用迁移跳过；
      001/002 全部 `CREATE ... IF NOT EXISTS` 以兼容无账本的存量库。
- [x] 抽出 `src/lib.rs`（库目标）+ 瘦 `main.rs`（CLI），使 `tests/` 能驱动真实 router；
      CLI 参数收敛到 `Args` 访问器，缺失参数打印 usage 而非下标越界。
- [x] `web.rs` 8 个资产处理器收敛为 `text_asset`/`binary_asset` 两个 helper。
- [x] 删除 `trading.rs` 7 个与 `models` 一一对应的 Response 结构体和映射函数：
      `models.rs` 直接 `#[derive(Serialize)]`，`Decimal` 序列化为规范化字符串。
- [x] `db.rs` 4 处「可选过滤参数」样板统一为 `collect_rows` + `(?1 IS NULL OR ...)`。
- [x] 前端：巨型单行 JSX 拆为具名子组件与常量表；`components.tsx` 归位为 `components/index.tsx`。

前端正确性与性能：

- [x] 修复 `useTrading` 的 TDZ 缺陷：`r` 快捷键在声明前引用 `refresh`。
- [x] 键盘监听由「每次渲染重新订阅」改为「订阅一次 + ref 读当前状态」。
- [x] 派生状态改为 `useMemo` + 单次 `summarize`：
      `contractActivity` 原为 O(合约×订单+持仓) 每渲染重算。
- [x] `App.tsx` 的 `onLogout` 用 `useCallback` 稳定化，
      否则每次 App 渲染都会触发一次 overview 重新请求。
- [x] 订单类型下拉的字段绑定重写，消除 id/label 错配风险。
- [x] 验证链接 token 改为惰性初值 + 仅 mount 消费一次，避免重复消费已用 token。
- [x] 移除内联 style，改为 `.verify-token-form` / `.verify-message` CSS 类。

测试：

- [x] 后端 18 个单元测试（原 9）。
- [x] 新增 `tests/api.rs` 19 个端到端测试：真实 socket + 临时 SQLite 库。
- [x] 前端 45 个测试。

### 2026-08-27 前端结构重构

- [x] 拆分 `src/main.tsx` 单体为 `api.ts` 客户端、`App.tsx`（App/Auth）、
      `hooks/useTrading.ts`（工作台状态与操作）、`components/Layout.tsx`
      （侧边栏/顶栏/导航/成交弹窗）与 `pages/Dashboard.tsx`，
      消除 props 钻取与单体堆积。

### 2026-08-25 交易 API 与前端接入

- [x] 确认数据库连接、wallet 和 systemd 服务正常。
- [x] 初始化 `USERS`、`SESSIONS` 认证表。
- [x] 预留固定密码的测试用户。
- [x] 为登录用户自动绑定一个模拟账户。
- [x] 增加账户、合约、订单、撤单、模拟成交、持仓和现金的 Web API。
- [x] 将交易数据接入前端 Dashboard。
- [x] 增加交易 API 的输入校验和基础测试。
- [x] 将内嵌原生前端迁移到 Vite + React + TypeScript。
- [x] 增加 PWA Manifest、图标、Service Worker 和移动端安全区域适配。
- [x] `/api/health` 增加真实数据库探活，失败返回 503。
- [x] 审查修复：订单参数先校验再分配单号，插入/提交失败时回滚事务。
- [x] 审查修复：总览按账户定点查询、合约存在性改为单行查询、撤单去掉前置全表扫描。
- [x] 审查修复：Web 处理器的同步数据库调用与 Argon2 哈希迁移到 `spawn_blocking`。
- [x] 审查修复：注册改为单条 INSERT 依赖唯一索引判定冲突；
      登录对未知邮箱执行等时 Argon2 校验。
- [x] 前端审查修复：总览现金指标只汇总账户基础币种；并发刷新加请求序号防乱序覆盖；
      401 自动登出；入金金额必须为正数。

> **历史说明**：这一阶段曾把 Web 层改成 Oracle 连接池，并为订单号分配加账户行锁。
> 随后的 SQLite 迁移把这两项都推翻了——单写者模型下连接池只会带来 `SQLITE_BUSY`，
> 而请求级 `Mutex<Connection>` 已经顺带串行化了订单号分配，行锁不再需要。
> 当前 `db::Pool` 的文档注释记录了这个取舍。

## 验收标准

以下条件应持续成立，改动相关代码时需要确认没有被破坏。

1. 登录用户只能看到并操作自己的模拟账户。
2. 前端可以查看账户、合约、订单、持仓和现金。
3. 前端可以提交订单、撤单，并注入模拟成交。
4. 订单和账本更新继续复用现有事务逻辑，不旁路写库。
5. 未配置 Resend 时不伪造「邮箱已验证」。
6. 同一个 `EXEC_ID` 重试不会重复记账。
7. 金额和数量不经过 `f64`；`185.52` 存进去读出来必须还是 `185.52`。
8. 停止服务（SIGTERM）不丢弃进行中的账本写入。
9. `ib init-db` 可重复执行；`ib init-auth` 不影响已有交易数据。
10. 平台不向任何真实市场发送订单。
