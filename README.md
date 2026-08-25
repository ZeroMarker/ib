# ib — IBKR 风格账户/订单/持仓数据库（Rust + Oracle）

用 Rust 在 Oracle 自治数据库（ADB）上复刻 Interactive Brokers 风格的
账户 / 订单 / 成交 / 持仓数据模型。连接方式与
`~/cloud/oracle/node-connection-test` 一致：mTLS wallet + TNS 别名。

## 数据模型

| 表 | 说明 |
| --- | --- |
| `CONTRACTS` | 合约：CONID、SYMBOL、SEC_TYPE（STK/OPT/FUT/CASH/BAG）、EXCHANGE、CURRENCY |
| `ACCOUNTS` | 账户：U 开头账号、CASH/MARGIN/IRA、状态 |
| `ORDERS` | 订单：ORDER_ID（客户端）、PERM_ID、BUY/SELL、MKT/LMT/STP/STP_LMT、TIF、IBKR 状态机（PendingSubmit → Submitted → Filled/Cancelled） |
| `FILLS` | 成交明细，按剩余数量成交并回写订单 |
| `POSITIONS` | 持仓快照（多头为正、空头为负）+ 平均成本 |
| `CASH_BALANCES` | 每币种现金余额 |

成交时会自动联动：更新订单成交量/均价/状态 → MERGE 更新持仓 →
按 BUY 减 / SELL 增更新对应币种现金。

## 构建

```bash
cargo build --release
```

运行需要 Oracle Instant Client：

```bash
./scripts/setup-instantclient.sh linux.arm64   # x86_64 用 linux.x64
export LD_LIBRARY_PATH=$HOME/instantclient/instantclient_23_26:$LD_LIBRARY_PATH
```

Ubuntu 24.04 的 `libaio` 改名为 `libaio.so.1t64`，需要补一个软链接：

```bash
ln -sf /usr/lib/aarch64-linux-gnu/libaio.so.1t64 \
    $HOME/instantclient/instantclient_23_26/libaio.so.1
```

另外钱包里的 `sqlnet.ora` 若使用 `?/network/admin` 占位符（Instant Client
没有 ORACLE_HOME，无法解析），需改为绝对路径：

```
WALLET_LOCATION = (SOURCE = (METHOD = file) (METHOD_DATA = (DIRECTORY=/home/ubuntu/oracle/wallet)))
```

## 连接配置

环境变量与 node 连接测试一致：

```bash
export DB_USER=APP_USER
export DB_PASSWORD=...
export DB_DSN=<tnsnames 别名>               # tnsnames.ora 里的别名
export DB_WALLET_DIR=/home/ubuntu/oracle/wallet
export TNS_ADMIN=$DB_WALLET_DIR             # 建议显式设置
```

注意：Rust 侧通过 client config dir 加载钱包（钱包目录即配置目录），
若钱包只有加密的 `ewallet.pem` 而没有 `cwallet.sso` 自动登录文件，
请先用 mkstore/wallet 生成 auto-login 版本。

## 使用示例

```bash
ib ping
ib init-db
ib account add U1234567 MARGIN
ib contract add 265598 AAPL STK SMART USD
ib order place 1 U1234567 265598 BUY LMT 100 185.50
ib fill add 1 U1234567 185.52        # 全部成交，联动持仓与现金
ib position list U1234567
ib cash list U1234567
```
