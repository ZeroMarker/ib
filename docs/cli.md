# CLI 参考

`ib` 二进制既是 Web 服务也是命令行工具。所有命令共享同一个数据库文件
（`DB_PATH`，默认 `./ib.sqlite3`）。

## 通用约定

退出码约定：

| 码 | 含义 |
| --- | --- |
| `0` | 成功 |
| `1` | 操作失败，stderr 打印 `ib: <操作> failed: <原因>` |
| `2` | 参数错误，打印完整 usage |

- **参数错误**统一走 usage + 退出码 `2`。位置参数经 `Args` 访问器读取，
  缺失的操作数打印 usage 而不是下标越界 panic；整数/小数解析失败同样走 usage。
- **数据层错误**打印单行消息后以 `1` 退出，不产生 Rust backtrace，
  `set -e` 脚本也能正常分支。例如重复账户是
  `ib: account failed: UNIQUE constraint failed: ACCOUNTS.ACCOUNT_ID`。
- 除 `serve` 外，所有命令直接打开连接操作文件；`serve` 走 `Pool`（单连接 + 互斥锁）。
- **没有 `--help` / `--version`。** 不带参数或传入未知命令会打印完整 usage 并以
  退出码 `2` 结束，这也是查看命令列表的方式。

共 17 个命令，分四组：服务与数据库管理、账户、合约、订单、成交、持仓、现金。

## 服务与数据库管理

### `ib serve [ADDR]`

启动 HTTP 服务，同时提供前端页面和 API。地址解析顺序：

1. 位置参数 `ADDR`
2. 环境变量 `SERVER_ADDR`
3. `127.0.0.1:8081`

收到 SIGINT（Ctrl-C）或 SIGTERM（systemd `stop`）后先排空连接再退出，
避免丢弃进行中的账本写入。

### `ib ping`

测试数据库连接，打印 SQLite 版本和实际打开的文件路径。

```
$ ib ping
simulation database ok: sqlite=3.50.4 path=./ib.sqlite3
```

### `ib init-db`

按文件名顺序执行 `migrations/*.sql`（001–004），并把已应用的迁移记入
`SCHEMA_MIGRATIONS`。**幂等**：已应用的迁移会跳过，可安全重复执行。
父目录不存在时自动创建。

### `ib init-auth`

只追加认证相关的迁移（002 用户与会话、003 邮箱验证、004 维护索引），**跳过 001
交易表**。用于给已有交易数据的库补上登录能力，不会触碰现有账本。

### `ib drop-db`

按外键依赖的逆序删除全部 10 张表（`SCHEMA_MIGRATIONS` 最后删）。
逐表打印 `dropped <TABLE>`；单表失败打印 `skip <TABLE>: <error>` 并继续。
**不可撤销。**

## 账户

### `ib account add <ACCOUNT_ID> [TYPE]`

`TYPE` 取 `CASH` / `MARGIN` / `IRA`（大小写不敏感，内部转大写），默认 `MARGIN`。
`ACCOUNT_ID` 是主键，需唯一；库层无长度约束（`user_account_id` 生成的 `SIM*` ID 为 16 字符）。
币种固定 `USD`，状态 `ACTIVE`。

### `ib account list`

按 `ACCOUNT_ID` 升序列出全部账户。

## 合约

### `ib contract add <CONID> <SYMBOL> <SEC_TYPE> [EXCHANGE] [CURRENCY]`

| 参数 | 必填 | 默认 | 说明 |
| --- | --- | --- | --- |
| `CONID` | 是 | — | 正整数，主键 |
| `SYMBOL` | 是 | — | 原样保存（不转大写） |
| `SEC_TYPE` | 否 | `STK` | `STK`/`OPT`/`FUT`/`CASH`/`BAG`/`IND`，转大写 |
| `EXCHANGE` | 否 | `SMART` | 转大写 |
| `CURRENCY` | 否 | `USD` | 转大写 |

合约乘数 `MULTIPLIER` 固定为 `1000000`（即 1.0），**CLI 无法设置**——
这决定了成交时现金 = 数量 × 价格 × 乘数 的缩放系数。

`(SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY)` 上有唯一约束，重复插入报错。

### `ib contract list`

按 `CONID` 升序列出全部合约。

## 订单

### `ib order place <ORDER_ID> <ACCOUNT_ID> <CONID> <SIDE> <ORDER_TYPE> <QTY> [LMT_PRICE] [AUX_PRICE]`

| 参数 | 说明 |
| --- | --- |
| `ORDER_ID` | **调用方指定**，与 HTTP API 自动分配不同 |
| `SIDE` | `BUY` / `SELL`，转大写 |
| `ORDER_TYPE` | `MKT` / `LMT` / `STP` / `STP_LMT`，转大写 |
| `QTY` | 正数，最多 6 位小数 |
| `LMT_PRICE` | `LMT`/`STP_LMT` 必填 |
| `AUX_PRICE` | `STP`/`STP_LMT` 必填，止损价 |

下单前 `validate_order` 逐条校验：类型枚举、数量为正、限价单必须有限价、
止损单必须有止损价、方向枚举、价格为正。落库状态固定为 `Submitted`，
`TIF` 取默认 `DAY`，`PERM_ID` 留空。

CLI **不预检合约是否存在**：外键约束（`FK_ORDERS_CONTRACT`）会在插入时报错。
先 `ib contract add` 再下单。

### `ib order list [STATUS]`

按 `ORDER_ID` 升序列出订单。`STATUS` 精确匹配，不区分大小写匹配由调用方保证
（SQL 是 `STATUS = ?`，库内统一存驼峰式如 `Submitted`、`Filled`）。

可选值：`PendingSubmit`、`Submitted`、`PreSubmitted`、`Filled`、`Cancelled`、`Inactive`。

### `ib order cancel <ORDER_ID> <ACCOUNT_ID>`

把状态改为 `Cancelled`，仅当当前状态不是 `Filled` / `Cancelled` 时生效。
不可撤时返回错误（含订单号和账户号）。

## 成交

### `ib fill add <ORDER_ID> <ACCOUNT_ID> <PRICE> [EXEC_ID]`

把订单**剩余数量一次性全部成交**，在一个事务里联动更新订单、持仓和现金。
这是当前唯一的撮合入口——平台没有行情源，注入价由调用方给定。

- `PRICE` 必须为正数，最多 6 位小数。
- `EXEC_ID` 长度 1–24 字节；省略时自动生成 `EX` + 16 位十六进制纳秒时间戳（18 字符）。
- **幂等**：同 `EXEC_ID` + 同订单 + 同账户 + 同价格重试是 no-op；
  `EXEC_ID` 被别的成交占用则报错。
- 订单必须处于 `Submitted` / `PreSubmitted` 且仍有剩余数量。

```bash
# 策略重试时复用同一个 EXEC_ID，不会重复记账
ib fill add 1 U1234567 185.52 EX-20260825-0001
```

现金变动 = ∓ 剩余数量 × 成交价 × 合约乘数（买入为负，卖出为正）。
持仓方向、符号与平均成本的处理规则见 [架构文档](architecture.md#成交幂等)。

## 持仓

### `ib position set <ACCOUNT_ID> <CONID> <POSITION> [AVG_COST]`

直接覆盖持仓快照。正数多头、负数空头；`POSITION` 为 0 时 `AVG_COST` 置空。
`AVG_COST` 省略或为 0 时存 `NULL`。**绕过成交逻辑直接改账**，用于造数或对账修正。

### `ib position list [ACCOUNT_ID]`

按账户、合约升序列出持仓。省略账户参数则列出全部。

## 现金

### `ib cash set <ACCOUNT_ID> <CURRENCY> <AMOUNT>`

直接覆盖余额（不是增量）。`CURRENCY` 转大写，3 字符。
`AMOUNT` 最多 6 位小数，**允许负数**（模拟透支/融资的预留口子）——
这与 HTTP 接口不同，`POST /api/trading/cash` 会拒绝负数，见 [API 文档](api.md)。
账户或币种不存在则新建行。

### `ib cash list [ACCOUNT_ID]`

按账户、币种升序列出余额，金额格式化为两位小数。省略账户参数则列出全部。

## 完整会话示例

```bash
ib init-db
ib account add U1234567 MARGIN
ib contract add 265598 AAPL STK SMART USD
ib cash set U1234567 USD 100000

ib order place 1 U1234567 265598 BUY LMT 100 185.50
ib fill add 1 U1234567 185.52 EX-20260825-0001

ib order list FILLED
ib position list U1234567
ib cash list U1234567
```

预期结果：订单 `Filled`，持仓 `100 @ 185.52`，现金 `81448`。
