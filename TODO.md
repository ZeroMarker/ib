# 项目待办与执行记录

更新时间：2026-09-29

## 当前结论

项目已经具备 SQLite 数据库连接、模拟交易账本、用户注册登录、终端式前端、Caddy 和 systemd 部署能力。质量门禁（CI、fmt/clippy、Prettier/ESLint/Vitest）与集成测试已就位，后续重点转向行情、资产估值和风控能力。

## 本轮执行（2026-09-29）：质量加固与重构

### 质量门禁

- [x] 新增 CI（`.github/workflows/ci.yml`）：后端 `fmt`/`clippy -D warnings`/测试，前端 `format:check`/`lint`/`test`/`build`；后端先构建前端产物（`include_str!` 依赖 `frontend/dist`）。
- [x] 前端接入 Prettier + ESLint 10（flat config、`typescript-eslint`、React Hooks、Vitest 5 + Testing Library），`npm run check` 一键跑格式/静态/测试。
- [x] 清理依赖漏洞：`npm audit` 0 vulnerabilities（升级到 eslint@10、vitest@5）。

### 真实缺陷修复

- [x] `email.rs` 每次发信都新建 `reqwest::Client` 且无超时：改为 `LazyLock` 单例共享连接池，并加 5s 连接 / 10s 请求超时，避免 Resend 挂起时请求永久阻塞。
- [x] 服务无优雅关停：`systemctl stop` 会直接丢弃进行中的账本写入。`axum::serve` 改为 `with_graceful_shutdown`，监听 SIGINT/SIGTERM 先排空连接再退出（已实测 SIGTERM 干净退出）。
- [x] `SESSIONS` / `EMAIL_VERIFICATIONS` 过期行只过滤不删除，表格无限增长：新增 `004_maintenance_indexes.sql` 与登录时的 `purge_expired` 清理。
- [x] 迁移按 `;` 朴素切分，字符串字面量或触发器体内的分号会破坏 SQL：改为跟踪引号跨度的切分器。
- [x] `resend-verification` 在未配置 Resend 时对已知邮箱返回 503、未知邮箱返回 200，构成账户枚举预言机：统一为通用 `ok`，失败只记日志。
- [x] clippy `result_large_err`：`Box<Response>` 被穿过数据层传递，改为小枚举 `ApiError`（`src/http.rs`），handler 可用 `?`。

### 结构重构

- [x] 迁移版本表 `SCHEMA_MIGRATIONS`：`init-db` 幂等，已应用迁移跳过；001/002 全部 `CREATE ... IF NOT EXISTS` 以兼容无账本的存量库。
- [x] 抽出 `src/lib.rs`（库目标）+ 瘦 `main.rs`（CLI），使 `tests/` 能驱动真实 router；CLI 参数收敛到 `Args` 访问器，缺失参数打印 usage 而非下标越界。
- [x] `web.rs` 8 个资产处理器收敛为 `text_asset`/`binary_asset` 两个helper（PNG 走字节路径）。
- [x] 删除 `trading.rs` 7 个与 `models` 一一对应的 Response 结构体和映射函数：`models.rs` 直接 `#[derive(Serialize)]`，`Decimal` 序列化为规范化字符串。
- [x] `db.rs` 4 处「可选过滤参数」样板统一为 `collect_rows` + `(?1 IS NULL OR ...)` 查询。
- [x] 前端：`PositionsPage`/`FillsPage`/`OrdersPage`/`OverviewPage`/`TradePage`/`Dashboard` 由单行巨型 JSX 拆为具名子组件与常量表；`components.tsx` 归位为 `components/index.tsx`。

### 前端正确性与性能

- [x] 修复 `useTrading` 的 TDZ 缺陷：`r` 快捷键在声明前引用 `refresh`，ESLint 报 `Cannot access variable before it is declared`。
- [x] 键盘监听由「每次渲染重新订阅」改为「订阅一次 + ref 读当前状态」。
- [x] 派生状态改为 `useMemo` + 单次 `summarize`：`contractActivity` 原为 O(合约×订单+持仓) 每渲染重算。
- [x] `App.tsx` 的 `onLogout` 用 `useCallback` 稳定化，否则每次 App 渲染都会触发一次 overview 重新请求。
- [x] 订单类型下拉的字段绑定重写（原 `isLimitOnly` 判断与 id/label 有错配风险），`STP_LMT` 限价字段 id 唯一。
- [x] 验证链接 token 改为惰性初值 + 仅 mount 消费一次，避免 effect 内同步 setState 与重复消费已用 token。
- [x] 移除内联 style，改为 `.verify-token-form` / `.verify-message` CSS 类。

### 测试

- [x] 后端 18 个单元测试（原 9）：迁移幂等与账本、迁移切分、过期行清理、序列化格式、静态资源 no-store 等。
- [x] 新增 `tests/api.rs` 19 个端到端测试：真实 socket + 临时 SQLite 库，覆盖注册/登录/登出、401 门禁、跨用户数据隔离、订单→成交→账本闭环、`exec_id` 幂等、撤单不可再成交、订单号按账户递增、过期会话回收。
- [x] 前端 45 个测试：API 客户端、路由派生、组件、订单/持仓/成交页、`useTrading` 汇总与校验。

## 历史执行（2026-08-27）

- [x] 前端结构重构：拆分 `src/main.tsx` 单体为 `api.ts` 客户端、`App.tsx`（App/Auth）、`hooks/useTrading.ts`（工作台状态与操作）、`components/Layout.tsx`（侧边栏/顶栏/导航/成交弹窗）与 `pages/Dashboard.tsx`，消除 props 钻取与单体堆积。

## 本轮执行

- [x] 确认数据库连接、wallet 和 systemd 服务正常。
- [x] 初始化 `USERS`、`SESSIONS` 认证表。
- [x] 预留固定密码的测试用户。
- [x] 为登录用户自动绑定一个模拟账户。
- [x] 增加账户、合约、订单、撤单、模拟成交、持仓和现金的 Web API。
- [x] 将交易数据接入前端 Dashboard。
- [x] 增加交易 API 的输入校验和基础测试。
- [x] 将内嵌原生前端迁移到 Vite + React + TypeScript。
- [x] 增加 PWA Manifest、图标、Service Worker 和移动端安全区域适配。
- [x] Web 服务改用可配置的 Oracle 连接池，避免全局单连接锁串行化请求。
- [x] `/api/health` 增加真实数据库探活，连接池获取或查询失败返回 503。
- [x] 订单号分配增加账户行锁，避免连接池并发下 `MAX(ORDER_ID) + 1` 冲突。
- [x] 审查修复：订单参数先校验再分配单号，插入/提交失败时回滚，避免池化连接带着未释放的账户行锁回池。
- [x] 审查修复：总览按账户定点查询、合约存在性改为单行查询、撤单去掉前置全表扫描。
- [x] 审查修复：Web 处理器的同步 Oracle 调用与 Argon2 哈希迁移到 `spawn_blocking`。
- [x] 审查修复：注册改为单条 INSERT 依赖唯一索引判定冲突；登录对未知邮箱执行等时 Argon2 校验。
- [x] 前端审查修复：总览现金指标只汇总账户基础币种；并发刷新加请求序号防乱序覆盖；401 自动登出；入金金额必须为正数。

## 外部依赖待办

- [x] Resend 邮箱验证：`RESEND_API_KEY` 已接通（`src/email.rs` 经 `POST https://api.resend.com/emails` 发信，`RESEND_FROM`/`APP_BASE_URL` 见 `deploy/ib.env.example`）；注册只建账户发邮件不签发会话（201 无 Cookie），已配置 Resend 时未验证登录返回 403，`POST /api/auth/verify` 验证（一次有效）、`POST /api/auth/resend-verification` 重发，前端注册后转登录提示验证、登录 403 给重发入口、验证横幅支持重发/粘贴验证码/链接自动验证。未配置密钥时注册照常成功、`email_verified` 保持 `false` 且登录门禁降级（防锁死）。已用真实密钥冒烟：注册无 Cookie→登录 403→验证→登录 200 有 Cookie→`me` 已验证；无密钥时登录 200 降级。
- [x] Resend 生产发信域名：`20070809.xyz` 已验证，生产 `RESEND_FROM=ib <verify@20070809.xyz>` + 真实密钥已写入 `/etc/ib/ib.env` 并重启，`APP_BASE_URL=https://ibkr.20070809.xyz`（线上 Caddy 实为 `ibkr` 子域名根路径，仓库 `deploy/Caddyfile` 已同步）。实测 `mark.chen.im@gmail.com` 重发 `ok`（日志无发送失败）、旧会话已清零，登录门禁全面生效。
- [ ] 行情驱动撮合：需要行情源或回测时钟，当前仍由 `fill add` 注入成交。
- [ ] 手续费、保证金和风控：需要明确模拟规则后实现。

## 后续增强

- [ ] 账户资产净值、未实现盈亏和行情快照。
- [x] 分页、筛选、订单详情和成交明细。
- [ ] 管理员与普通用户权限分离。
- [x] 数据库迁移版本表和可重复执行迁移（`SCHEMA_MIGRATIONS` + 幂等 DDL）。
- [x] 集成测试：临时 SQLite 库、登录态、订单到成交账务闭环（`tests/api.rs`，19 例）。
- [x] 将同步数据库调用迁移到 `spawn_blocking`，避免慢查询占用 Tokio 工作线程。

## 前端功能待办

### 本轮体验升级

- [x] 增加终端侧边导航和总览信息架构。
- [x] 增加账户会话快照、系统状态和模拟环境安全提示。
- [x] 增加合约观察卡片、交易入口和账户数据视觉层级。
- [x] 完善桌面端与移动端响应式布局。
- [x] 将总览、交易、订单、持仓、成交拆分为独立 URL 视图。
- [x] 将各业务页面抽离为独立 TypeScript 页面组件，避免继续堆积在单文件。

### P0：本轮执行

- [x] 订单状态筛选、分页和详情展开。
- [x] 支持 MKT/LMT/STP/STP_LMT 的完整价格参数。
- [x] 表单输入校验、提交中状态和防重复提交。
- [x] 合约搜索和前端重复选择提示。
- [x] 统一网络错误提示、重试和数据刷新时间。

### P1：需要后端配合

- [ ] 行情自选列表和价格卡片。
- [ ] 持仓市值、未实现盈亏和行情时间戳。
- [ ] 手续费、保证金和风控状态展示。
- [ ] 多币种资产汇总和账户设置。
- [ ] 订单成交明细与审计时间线。

### P2：产品增强

- [ ] 多模拟账户切换和资金划转。
- [ ] 策略运行监控和回测结果页面。
- [ ] 深色模式、快捷键和浏览器通知。

## 验收标准

1. 登录用户只能看到并操作自己的模拟账户。
2. 前端可以查看账户、合约、订单、持仓和现金。
3. 前端可以提交订单、撤单，并注入模拟成交。
4. 订单和账本更新继续复用现有事务逻辑。
5. 未配置 Resend 时不伪造“邮箱已验证”。
