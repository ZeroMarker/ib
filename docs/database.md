# 数据模型

单个 SQLite 文件。默认路径 `./ib.sqlite3`，可用 `DB_PATH` 覆盖。
无模式类型，所有列都是 `TEXT` / `INTEGER`，没有 `REAL`——金额和数量一律是定点整数。

## 连接参数

`db::open()` 在每次打开时设置（`src/db.rs`）：

| PRAGMA | 值 | 原因 |
| --- | --- | --- |
| `journal_mode` | `WAL` | 读写不互相阻塞 |
| `busy_timeout` | 5000 ms | 遇到写锁最多等 5 秒而不是立即 `SQLITE_BUSY` |
| `foreign_keys` | `ON` | SQLite 默认**关闭**外键约束，必须显式开 |

父目录不存在会自动创建。服务运行期只持有**一个**连接
（`Pool = Mutex<Connection>`），所以 `busy_timeout` 实际上只对 CLI 进程和测试有意义。

## 定点约定

所有金额和数量列都是 `INTEGER`，含义为**微单位**：`1_000_000` = 1 单位。
比例常数 `DECIMAL_SCALE = 1_000_000` 定义在 `src/db.rs`。

| 表.列 | 含义 |
| --- | --- |
| `POSITIONS.POSITION` | 带符号微单位持仓，正多头负空头 |
| `POSITIONS.AVG_COST` | 平均成本微单位，平仓时置 `NULL` |
| `CASH_BALANCES.CASH` | 余额微单位，允许负数 |
| `ORDERS.TOTAL_QUANTITY` / `FILLED_QUANTITY` | 数量微单位 |
| `ORDERS.LMT_PRICE` / `AUX_PRICE` / `AVG_FILL_PRICE` | 价格微单位 |
| `FILLS.QUANTITY` / `PRICE` | 数量与价格微单位 |
| `CONTRACTS.MULTIPLIER` | 合约乘数微单位，默认 `1000000`（即 1.0） |

换算规则（`src/db.rs`）：

- `scaled()` 用于**用户输入**：小数位超过 6 位直接**报错**，不四舍五入。
- `scaled_round()` 用于**派生值**（平均成本混合、现金差额）：先 `round_dp(6)` 再缩放。
  算术结果静默舍入比报错合理。

详见[架构文档](architecture.md#定点整数微单位与浮点的区别)。

## 交易表（迁移 001）

### `CONTRACTS` —— 合约参考数据，全站共享

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `CONID` | INTEGER | 主键 |
| `SYMBOL` | TEXT | NOT NULL |
| `SEC_TYPE` | TEXT | NOT NULL，CHECK ∈ `STK`/`OPT`/`FUT`/`CASH`/`BAG`/`IND` |
| `EXCHANGE` | TEXT | NOT NULL，默认 `SMART` |
| `CURRENCY` | TEXT | NOT NULL，默认 `USD` |
| `LOCAL_SYMBOL` | TEXT | 可空，当前代码未使用 |
| `MULTIPLIER` | INTEGER | NOT NULL，默认 `1000000`，CHECK > 0 |
| `CREATED_AT` | TEXT | NOT NULL，默认 `datetime('now')` |

唯一约束：`UQ_CONTRACTS (SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY)`。

合约是**共享**的静态数据，不归属任何账户。`GET /api/trading/overview` 因此返回全站合约。

### `ACCOUNTS` —— 模拟账户

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `ACCOUNT_ID` | TEXT | 主键，无长度约束 |
| `ACCOUNT_TYPE` | TEXT | NOT NULL，默认 `MARGIN`，CHECK ∈ `CASH`/`MARGIN`/`IRA` |
| `CURRENCY` | TEXT | NOT NULL，默认 `USD` |
| `STATUS` | TEXT | NOT NULL，默认 `ACTIVE`，CHECK ∈ `ACTIVE`/`CLOSED`/`RESTRICTED` |
| `CREATED_AT` | TEXT | NOT NULL |

HTTP 用户通过 `user_account_id(user_id)` 得到稳定的 `SIM<13 位十六进制>` 账户。

### `ORDERS` —— 订单

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `PERM_ID` | INTEGER | 可空，接受后由外部分配；当前代码不写 |
| `ORDER_ID` | INTEGER | **主键的一部分**，调用方指定或按账户分配 |
| `ACCOUNT_ID` | TEXT | 外键 → `ACCOUNTS`，主键的一部分 |
| `CONID` | INTEGER | 外键 → `CONTRACTS` |
| `SIDE` | TEXT | CHECK ∈ `BUY`/`SELL` |
| `ORDER_TYPE` | TEXT | CHECK ∈ `MKT`/`LMT`/`STP`/`STP_LMT` |
| `LMT_PRICE` | INTEGER | 可空 |
| `AUX_PRICE` | INTEGER | 可空，止损价 |
| `TOTAL_QUANTITY` | INTEGER | NOT NULL，CHECK > 0 |
| `FILLED_QUANTITY` | INTEGER | NOT NULL，默认 0，CHECK ≥ 0 且 ≤ `TOTAL_QUANTITY` |
| `AVG_FILL_PRICE` | INTEGER | 可空，CHECK 为空或 > 0 |
| `TIF` | TEXT | NOT NULL，默认 `DAY`，CHECK ∈ `DAY`/`GTC`/`IOC`/`OPG` |
| `OUTSIDE_RTH` | INTEGER | NOT NULL，默认 0 |
| `STATUS` | TEXT | NOT NULL，默认 `PendingSubmit`，CHECK 见下 |
| `PARENT_ORDER_ID` | INTEGER | 可空，括号单用，当前未使用 |
| `CREATED_AT` / `UPDATED_AT` | TEXT | NOT NULL |

**主键是 `(ORDER_ID, ACCOUNT_ID)`**，不是 `ORDER_ID` 单独。所以订单号只需在账户内唯一，
`next_order_id` 也按账户做 `MAX(ORDER_ID) + 1`。

**状态取值**：`PendingSubmit`、`Submitted`、`PreSubmitted`、`Filled`、`Cancelled`、`Inactive`。
当前代码只会写入 `Submitted`（下单）、`Filled`（成交）、`Cancelled`（撤单），
其余值是为兼容券商术语预留的。

唯一约束 `UQ_ORDERS_FILL_REF (ORDER_ID, ACCOUNT_ID, CONID, SIDE)`
供 `FILLS` 的复合外键引用。

### `FILLS` —— 成交明细

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `EXEC_ID` | TEXT | **主键**，1–24 字节，全局唯一 |
| `ORDER_ID` / `ACCOUNT_ID` | INTEGER / TEXT | 复合外键 → `ORDERS (ORDER_ID, ACCOUNT_ID)` |
| `CONID` / `SIDE` | INTEGER / TEXT | 复合外键 → `ORDERS (ORDER_ID, ACCOUNT_ID, CONID, SIDE)` |
| `QUANTITY` | INTEGER | NOT NULL，CHECK > 0 |
| `PRICE` | INTEGER | NOT NULL，CHECK > 0 |
| `EXEC_TIME` | TEXT | NOT NULL，默认 `datetime('now')` |

`EXEC_ID` 是主键，这是**成交幂等**的物理基础：同一个执行 ID 不可能记账两次。

### `POSITIONS` —— 持仓快照

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `ACCOUNT_ID` | TEXT | 主键一部分，外键 → `ACCOUNTS` |
| `CONID` | INTEGER | 主键一部分，外键 → `CONTRACTS` |
| `POSITION` | INTEGER | NOT NULL，带符号微单位 |
| `AVG_COST` | INTEGER | 可空 |
| `MARKET_PRICE` | INTEGER | 可空，**预留列，当前代码不写** |
| `UPDATED_AT` | TEXT | NOT NULL |

主键 `(ACCOUNT_ID, CONID)`——没有币种维度，所以同一合约的多币种持仓无法表达。
这是 P1「多币种资产汇总」要解决的问题。

### `CASH_BALANCES` —— 现金账本

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `ACCOUNT_ID` | TEXT | 主键一部分，外键 → `ACCOUNTS` |
| `CURRENCY` | TEXT | 主键一部分 |
| `CASH` | INTEGER | NOT NULL，默认 0，**允许负数** |
| `UPDATED_AT` | TEXT | NOT NULL |

写入一律用 `INSERT ... ON CONFLICT (ACCOUNT_ID, CURRENCY) DO UPDATE`，不存在则建行。

## 认证表（迁移 002、003）

### `USERS`

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `USER_ID` | TEXT | 主键，UUID v4 |
| `EMAIL` | TEXT | NOT NULL，**唯一**，存储去空格 + 转小写后的值 |
| `PASSWORD_HASH` | TEXT | NOT NULL，Argon2 PHC 字符串 |
| `EMAIL_VERIFIED` | INTEGER | NOT NULL，默认 0，CHECK ∈ {0, 1} |
| `STATUS` | TEXT | NOT NULL，默认 `ACTIVE`，CHECK ∈ `ACTIVE`/`DISABLED` |
| `CREATED_AT` | TEXT | NOT NULL |

邮箱归一化（`trim` + `to_lowercase`）后存库，唯一索引因此大小写不敏感。
`normalize_email` 额外要求：≤320 字符、含 `@`、本地部分非空、域名非空且含 `.`。

### `SESSIONS`

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `SESSION_ID` | TEXT | 主键，UUID v4 |
| `USER_ID` | TEXT | 外键 → `USERS` |
| `TOKEN_HASH` | TEXT | NOT NULL，**唯一**，token 的 SHA-256 十六进制 |
| `EXPIRES_AT` | TEXT | NOT NULL，创建时 `+30 days` |
| `CREATED_AT` | TEXT | NOT NULL |

明文 token 只存在于 Cookie，库里只存哈希。会话有效性由 `EXPIRES_AT > datetime('now')`
在每次查询时判定，不需要定时任务。

### `EMAIL_VERIFICATIONS`

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `VERIFICATION_ID` | TEXT | 主键，UUID v4 |
| `USER_ID` | TEXT | 外键 → `USERS` |
| `TOKEN_HASH` | TEXT | NOT NULL，**唯一**，SHA-256 |
| `EXPIRES_AT` | TEXT | NOT NULL，创建时 `+24 hours` |
| `CREATED_AT` | TEXT | NOT NULL |

**每用户一行**：重发验证会先删掉旧行，所以只有最新发出的链接有效。
验证成功时删除该用户全部行。

> 出于兼容老库的目的，`auth::ensure_verification_table()` 在写入前还会
> `CREATE TABLE IF NOT EXISTS` 一次。迁移 003 之前的数据库因此无需手动重新迁移。

### `SCHEMA_MIGRATIONS` —— 迁移账本

| 列 | 类型 | 约束 |
| --- | --- | --- |
| `NAME` | TEXT | 主键，迁移文件名 |
| `APPLIED_AT` | TEXT | NOT NULL，默认 `datetime('now')` |

由 `db::ensure_migration_ledger()` 隐式创建，不在 `migrations/` 里。

## 索引

| 索引 | 表.列 | 服务的查询 |
| --- | --- | --- |
| `IX_FILLS_ORDER` | `FILLS (ORDER_ID, ACCOUNT_ID)` | 按订单查成交 |
| `IX_SESSIONS_USER` | `SESSIONS (USER_ID)` | 用户会话查询/删除 |
| `IX_EMAIL_VERIFICATIONS_USER` | `EMAIL_VERIFICATIONS (USER_ID)` | 重发时删旧 token、验证后清空 |
| `IX_SESSIONS_EXPIRES_AT` | `SESSIONS (EXPIRES_AT)` | 过期行清理 |
| `IX_EMAIL_VERIFICATIONS_EXPIRES_AT` | `EMAIL_VERIFICATIONS (EXPIRES_AT)` | 过期行清理 |
| `IX_ORDERS_ACCOUNT_STATUS` | `ORDERS (ACCOUNT_ID, STATUS)` | 总览按账户列订单、按状态过滤 |
| `IX_FILLS_ACCOUNT_TIME` | `FILLS (ACCOUNT_ID, EXEC_TIME DESC)` | 总览取最近成交 |

`IX_FILLS_ORDER` 来自 001，`IX_SESSIONS_USER` / `IX_EMAIL_VERIFICATIONS_USER` 来自 002/003，
其余 4 个来自 004。`ORDERS` / `FILLS` / `POSITIONS` 永远只按账户查询，
所以复合索引都以 `ACCOUNT_ID` 打头。

**表增长治理**：`SESSIONS` 和 `EMAIL_VERIFICATIONS` 早期只过滤不删除，会无限增长。
现在每次登录调用 `auth::purge_expired()` 删掉过期行（失败只记日志，绝不阻断登录）。
迁移 004 补的两个 `EXPIRES_AT` 索引就是让这个清理不退化成全表扫描。

## 迁移机制

### 文件

`migrations/*.sql` 通过 `include_str!` 编译进二进制（`db::MIGRATIONS`），
按数组顺序（当前即文件名顺序）执行：

| 文件 | 内容 |
| --- | --- |
| `001_simulation_schema.sql` | `CONTRACTS`、`ACCOUNTS`、`ORDERS`、`FILLS`、`POSITIONS`、`CASH_BALANCES` |
| `002_auth_schema.sql` | `USERS`、`SESSIONS` |
| `003_email_verification.sql` | `EMAIL_VERIFICATIONS` |
| `004_maintenance_indexes.sql` | 过期清理与总览查询所需的 4 个索引 |

### 执行规则

1. 先确保 `SCHEMA_MIGRATIONS` 存在。
2. 逐个检查 `NAME` 是否已记录，已记录则跳过。
3. 用 `split_statements()` 切分后逐条 `execute_batch`。
4. **全部语句成功后才写入账本**——失败保持 pending，下次运行重试。

### SQL 切分器

不是朴素的 `split(';')`。`split_statements()` 处理三类会被切坏的情况：

1. **字符串字面量里的分号。** 跟踪单引号和双引号跨度。
2. **注释里的分号。** 引号外的 `--` 注释行整行丢弃。
3. **`BEGIN ... END` 触发器体。** 维护块深度，只在深度 0 处切分。
   `BEGIN` / `END` 只在作为独立单词时识别（`APPENDEND`、`BEGINNER` 不算），
   且闭合的 `END` 必须**独占一行**——否则 `CASE ... END` 会被误判成块结束。

`public/sw.js` 那类「构建期注入」的做法在这里不适用：切分器必须能识别
触发器体的分号，所以它理解了块结构，而不是把它当不透明文本。

### 命令对应

| 命令 | 执行的迁移 |
| --- | --- |
| `ib init-db` | 001–004 全部 |
| `ib init-auth` | 002–004（跳过 001） |
| `ib drop-db` | 无（按依赖逆序 `DROP TABLE IF EXISTS` 全部 10 张表） |

001 的每条语句都是 `CREATE ... IF NOT EXISTS`，
让「迁移账本出现之前创建的存量库」也能安全重复执行 `init-db`。

## 备份

WAL 模式下直接 `cp ib.sqlite3` 是不安全的（WAL 尚在 `-wal` 文件里）。
使用 SQLite 自身的备份接口，或停服后连 `-wal` / `-shm` 一起复制。
`.gitignore` 已排除 `*.sqlite3` / `*.sqlite3-wal` / `*.sqlite3-shm`。
