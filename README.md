# ib — 模拟交易平台（Rust + Oracle）

`ib` 是一个面向策略开发和纸上交易的模拟交易平台。它提供账户、合约、订单、模拟成交、持仓和现金账本，用 Oracle 保存状态，用 Rust 提供 CLI 操作入口。

项目定位是模拟交易和交易账务演练，不是券商客户端：不会连接 Interactive Brokers 下单，也不会把任何订单发送到真实市场。当前的撮合入口是 `fill add`，由测试程序或策略适配器注入成交价；后续可以在此基础上接入行情、撮合规则、风控和回测时钟。

## 能力边界

| 能力 | 当前支持 |
| --- | --- |
| 账户与合约 | 创建、查询账户和交易合约 |
| 模拟订单 | MKT/LMT/STP/STP_LMT，BUY/SELL，订单状态管理 |
| 模拟成交 | 按订单剩余数量注入一次完整成交 |
| 持仓账本 | 多空方向、平均成本、合约乘数 |
| 现金账本 | 按账户和币种维护现金余额 |
| 真实交易 | 不支持，不连接券商交易 API |

成交会在一个事务中联动更新订单、持仓和现金。成交 `EXEC_ID` 具有幂等性，重试同一模拟成交不会重复记账。

## 数据模型

| 表 | 说明 |
| --- | --- |
| `CONTRACTS` | 合约标识、证券类型、交易所、币种和乘数 |
| `ACCOUNTS` | 模拟账户、账户类型和状态 |
| `ORDERS` | 模拟订单、价格、数量、有效期和状态 |
| `FILLS` | 模拟成交明细和执行 ID |
| `POSITIONS` | 每账户/合约的多空持仓和平均成本 |
| `CASH_BALANCES` | 每账户/币种的现金余额 |

资金和数量最多支持 6 位小数，应用层使用 Decimal，数据库使用 `NUMBER(18,6)`。

## 构建

```bash
cargo build --release
```

运行需要 Oracle Instant Client：

```bash
./scripts/setup-instantclient.sh linux.arm64   # x86_64 使用 linux.x64
export LD_LIBRARY_PATH=$HOME/instantclient/instantclient_23_26:$LD_LIBRARY_PATH
```

Ubuntu 24.04 的 `libaio` 改名为 `libaio.so.1t64`，需要补一个软链接：

```bash
ln -sf /usr/lib/aarch64-linux-gnu/libaio.so.1t64 \
    $HOME/instantclient/instantclient_23_26/libaio.so.1
```

## 数据库连接

```bash
export DB_USER=APP_USER
export DB_PASSWORD=...
export DB_DSN=<tnsnames 别名>
export DB_WALLET_DIR=/home/ubuntu/oracle/wallet
export TNS_ADMIN=$DB_WALLET_DIR
```

钱包目录需要 `tnsnames.ora`，以及 `cwallet.sso`、`ewallet.pem` 或 `ewallet.p12` 之一。

## 模拟交易示例

```bash
ib ping
ib init-db
ib account add U1234567 MARGIN
ib contract add 265598 AAPL STK SMART USD
ib cash set U1234567 USD 100000
ib order place 1 U1234567 265598 BUY LMT 100 185.50

# 注入模拟成交；策略重试时复用同一个 EXEC_ID
ib fill add 1 U1234567 185.52 EX-20260825-0001

ib order list FILLED
ib position list U1234567
ib cash list U1234567
```

`fill add` 当前会把订单剩余数量一次性成交，适合作为最小纸上交易闭环。自动撮合、行情驱动成交、手续费、保证金和风控尚未纳入当前版本。

新数据库使用 `ib init-db` 创建全部交易和认证表。已有交易数据库可先执行 `ib init-auth`，只追加用户和会话表，不会触碰现有交易数据。

## 用户注册与登录

启动 HTTP API：

```bash
export SERVER_ADDR=127.0.0.1:8081
ib serve
```

接口包括：

- `POST /api/auth/register`：`{"email":"user@example.com","password":"..."}`
- `POST /api/auth/login`：登录并设置 HttpOnly 会话 Cookie
- `POST /api/auth/logout`：注销当前会话
- `GET /api/auth/me`：读取当前登录用户
- `GET /api/health`：健康检查

密码使用 Argon2 哈希，会话只在数据库保存令牌哈希。邮箱验证发送预留了 Resend TODO；在邮件适配器完成前，新用户可以直接登录，但 `email_verified` 为 `false`。

Resend 接入完成后通过 `RESEND_API_KEY` 配置密钥；当前版本不会读取或发送邮件。

前端页面由同一个 Rust 服务提供，包含登录、注册、登录态恢复、用户信息和退出登录视图。直接访问 `/`，或通过 Caddy 访问 `/public/ibkr/` 即可打开页面，不需要额外的 Node.js 构建步骤。

## Caddy 部署

`deploy/Caddyfile` 将 `https://20070809.xyz/public/ibkr/` 反向代理到本机的 `127.0.0.1:8081`，并移除外部路径前缀。启动服务后将该配置加入 Caddy：

```bash
caddy validate --config deploy/Caddyfile
caddy reload --config deploy/Caddyfile
```

## systemd 部署

`deploy/ib.service` 以 `ubuntu` 用户运行 release 二进制，监听 `127.0.0.1:8081`，适合配合上面的 Caddy 配置。

```bash
cargo build --release
sudo install -d -m 0750 /etc/ib
sudo install -o root -g ubuntu -m 0640 deploy/ib.env.example /etc/ib/ib.env
sudoedit /etc/ib/ib.env

sudo install -m 0644 deploy/ib.service /etc/systemd/system/ib.service
sudo systemctl daemon-reload
sudo systemctl enable --now ib.service
sudo systemctl status ib.service
```

查看日志或重启：

```bash
journalctl -u ib.service -f
sudo systemctl restart ib.service
```
