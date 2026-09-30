# ib — 模拟交易平台（Rust + SQLite）

`ib` 是一个面向策略开发和纸上交易的模拟交易平台。它提供账户、合约、订单、模拟成交、
持仓和现金账本，用 SQLite 保存状态，用 Rust 提供 CLI 与 HTTP 服务，前端由同一个二进制提供。

**项目定位是模拟交易和交易账务演练，不是券商客户端**：不会连接 Interactive Brokers 下单，
也不会把任何订单发送到真实市场。唯一的外部依赖是可选的 Resend，仅用于发送邮箱验证邮件。

## 能力边界

| 能力 | 当前支持 |
| --- | --- |
| 账户与合约 | 创建、查询模拟账户和交易合约 |
| 模拟订单 | MKT/LMT/STP/STP_LMT，BUY/SELL，状态管理、按账户分配订单号 |
| 模拟成交 | 按订单剩余数量注入一次完整成交，`EXEC_ID` 幂等 |
| 持仓账本 | 多空方向、平均成本、合约乘数 |
| 现金账本 | 按账户和币种维护现金余额 |
| 用户体系 | 邮箱注册、Argon2 密码、会话 Cookie、邮箱验证 |
| 撮合 / 行情 | **无**。成交价由调用方注入，没有行情源或回测时钟 |
| 手续费 / 保证金 / 风控 | **无** |
| 真实交易 | **不支持**，不连接券商交易 API |

成交会在一个事务中联动更新订单、持仓和现金。成交 `EXEC_ID` 具有幂等性，
重试同一模拟成交不会重复记账。

## 快速开始

构建顺序不能颠倒：`src/web.rs` 用 `include_str!` 嵌入 `frontend/dist`，
所以前端产物必须先于后端编译产出。

```bash
cd frontend
npm ci
npm run build
cd ..

cargo build --release

export DB_PATH=./ib.sqlite3
./target/release/ib init-db
./target/release/ib serve
```

打开 <http://localhost:8081>，注册一个账户即可。
用 `localhost` 而非 `127.0.0.1` 或局域网 IP——会话 Cookie 带 `Secure` 标记。

无需安装任何数据库客户端：SQLite 以 `bundled` 特性静态编译进二进制。

### CLI 快速上手

```bash
ib init-db
ib account add U1234567 MARGIN
ib contract add 265598 AAPL STK SMART USD
ib cash set U1234567 USD 100000
ib order place 1 U1234567 265598 BUY LMT 100 185.50
ib fill add 1 U1234567 185.52 EX-20260825-0001   # 同 EXEC_ID 重试不会重复记账
ib order list FILLED
ib position list U1234567
ib cash list U1234567
```

完整命令参考见 [docs/cli.md](docs/cli.md)。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | 进程拓扑、构建顺序约束、模块地图、请求生命周期、关键设计决策、安全姿态 |
| [docs/cli.md](docs/cli.md) | 全部 17 个 CLI 命令的参数、行为与坑 |
| [docs/api.md](docs/api.md) | HTTP API 参考：端点、请求/响应、状态码、静态资源、完整 curl 示例 |
| [docs/database.md](docs/database.md) | 10 张表的完整列与约束、定点约定、索引、迁移机制 |
| [docs/frontend.md](docs/frontend.md) | 前端结构、状态管理约定、视图与路由、快捷键、PWA、已知限制 |
| [docs/deployment.md](docs/deployment.md) | 环境变量、本地运行、Caddy、systemd、升级流程 |
| [docs/testing.md](docs/testing.md) | 质量门禁命令、CI 编排、98 个测试的完整清单与未覆盖区域 |
| [TODO.md](TODO.md) | 待办与执行记录 |

## 技术要点

- **单文件 SQLite，无外部服务。** 唯一的可选依赖是 Resend，未配置时功能降级但不报错。
- **定点整数微单位。** 金额和数量在库中是 `INTEGER`（`1_000_000` = 1 单位），
  应用层用 `Decimal`，全程不经过 `f64`，`185.52` 精确往返。
- **单连接 + 互斥锁。** SQLite 天然单写者；请求级持锁顺带串行化了订单号分配，
  所以 `next_order_id` 不需要行锁。
- **同步工作离开 reactor。** SQLite 查询和 Argon2 哈希全部走 `spawn_blocking`。
- **优雅关停。** SIGINT / SIGTERM 先排空连接再退出，不丢进行中的账本写入。
- **幂等且不伪造状态。** 同一 `EXEC_ID` 重试是 no-op；未配置邮件服务时
  `email_verified` 恒为 `false`，绝不假装已验证。
- **校验不靠前端。** 入金金额正数等业务规则在服务端强制执行，浏览器检查只是体验优化。
- **契约共享。** `api-schema.json` 同时约束 Rust 的序列化字段和前端的类型键集。

未完成的能力（撮合、行情、手续费、风控、限流）见 [TODO.md](TODO.md#待办)。

## 许可

见 [LICENSE](LICENSE)。
