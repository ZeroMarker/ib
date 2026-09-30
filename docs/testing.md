# 质量门禁与测试

## 命令汇总

### 后端

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib          # 23 个单元测试
cargo test --test api     # 20 个端到端测试
```

### 前端

```bash
cd frontend
npm run format:check      # prettier --check
npm run lint              # eslint
npm run test              # vitest run，50 个测试（含 5 个契约测试）
npm run build             # tsc -b && vite build，兼做类型检查

npm run check             # format:check + lint + test 三合一
```

`npm run check` **不含** `tsc`。类型检查只在 `npm run build` 里发生，
所以提交前跑一次 `npm run build`。

### 完整门禁

`.github/workflows/ci.yml` 在 `main` 分支 push 和所有 PR 上跑两个 job：

| Job | 步骤 |
| --- | --- |
| `backend` | checkout → Rust stable（rustfmt + clippy）→ cargo 缓存 → **构建前端** → fmt → clippy → `--lib` 测试 → `--test api` 测试 |
| `frontend` | checkout → Node 22（npm 缓存）→ `npm ci` → format:check → lint → test → build |

`backend` job 在 fmt 之前必须先 `npm ci && npm run build`：
`src/web.rs` 的 `include_str!` 依赖 `frontend/dist` 存在，否则后端根本编译不过。

集成测试单独一个 step 而不是与单元测试合并，是为了在 CI 日志里区分失败来源
（它会绑定真实 socket 并打开真实 SQLite 文件）。

## 编译期约束

`Cargo.toml` 的 `[lints]` 段把约束前移到编译：

```toml
[lints.rust]
unsafe_code = "forbid"      # 全项目零 unsafe

[lints.clippy]
all = { level = "warn", priority = -1 }
```

配合 CI 的 `-D warnings`，任何 clippy 提示都会让 CI 失败。

`Cargo.toml` 里还有两处注释解释了不能随手改的决策：
`[lints.clippy]` 说明 `ApiError` 为何取代 `Box<Response>`；
`[dev-dependencies]` 说明测试客户端为何手写 `TcpStream` 而不引入 HTTP 客户端依赖。
Release profile 开启 `lto = "thin"` + `codegen-units = 1`，注释指明理由是
定点缩放和总览 JSON 编码这两条热路径的跨 crate 内联。

## 测试覆盖清单

### 单元测试（`cargo test --lib`，23 个）

#### `src/db.rs`（12）

| 测试 | 覆盖 |
| --- | --- |
| `fixed_point_values_are_scaled_without_binary_rounding` | `185.50` → `185_500_000`；`0.000001` → `1`，不经二进制浮点 |
| `values_with_more_than_six_places_are_rejected_for_input` | 输入路径 `scaled` 拒绝 7 位小数；派生路径 `scaled_round` 正常舍入 |
| `user_account_id_is_stable_and_fits_account_limit` | UUID → `SIM9eab52263a104`，长度 16 |
| `migration_statements_survive_quotes_and_comments` | 切分器不被 `'x;y'` 里的分号和 `--` 注释破坏 |
| `trigger_bodies_stay_in_one_statement` | `CREATE TRIGGER ... BEGIN ... END;` 的整个函数体（含内部分号和 `END`）保持为一条语句 |
| `trigger_keywords_must_standalone` | 列名 `APPENDEND`、表名 `BEGINNER` 和散文注释里的 `BEGIN` 不被当成块边界 |
| `a_case_expression_inside_a_trigger_does_not_close_the_block` | 触发器体内的 `CASE ... END` 不提前闭合块 |
| `init_schema_is_idempotent_and_records_a_ledger` | 账本条数正确；重复执行不报错也不重跑 |
| `init_auth_schema_adds_login_tables_without_dropping_trading_data` | 模拟「无账本的存量库」状态，`init-auth` 补表且现金数据不动 |
| `expired_session_and_verification_rows_are_pruned` | `purge_expired` 只留未过期会话，清空过期验证码 |
| `sqlite_round_trip_covers_order_fill_position_and_cash` | 下单→成交→持仓/现金闭环；同 `EXEC_ID` 重试幂等；185.52 精确往返 |
| `order_ids_allocate_per_account_without_a_row_lock` | 按账户 `MAX+1` 递增 |

#### `src/auth.rs`（2）

| 测试 | 覆盖 |
| --- | --- |
| `email_is_normalized_and_validated` | 归一化（去空格 + 转小写）；拒绝无 `@` 的输入 |
| `passwords_are_hashed_and_verified` | Argon2 哈希可验证；错误密码被拒 |

#### `src/email.rs`（3）

| 测试 | 覆盖 |
| --- | --- |
| `verification_link_appends_token` | `APP_BASE_URL` 结尾斜杠被正确处理；测试后恢复环境变量 |
| `verification_sender_reuses_a_single_client` | `LazyLock` 单例，按指针地址断言 |
| `verification_bodies_carry_link_and_token` | HTML / 纯文本正文都带链接和验证码 |

#### `src/models.rs`（4）

| 测试 | 覆盖 |
| --- | --- |
| `decimals_serialize_as_normalized_strings` | `"185.520000"` → `"185.52"`，`"100.000000"` → `"100"`，空值为 `null` |
| `negative_positions_keep_their_sign` | 空头保留负号，空均价序列化为 `"3"` |

#### `src/web.rs`（2）

| 测试 | 覆盖 |
| --- | --- |
| `the_wire_contract_matches_this_file` | 6 个模型类型的序列化字段与 `api-schema.json` 完全一致 |
| `the_user_response_matches_the_contract` | `auth::UserResponse` 字段与契约一致 |
| `static_assets_are_served_no_store` | 8 个资源的 `Cache-Control` 与 `Content-Type` 齐全 |
| `binary_icons_keep_a_non_utf8_content_type` | PNG 字节（`0x89` 前缀）走 `&[u8]` 路径不被当作 UTF-8 拒绝 |

### 端到端测试（`cargo test --test api`，20 个）

`tests/api.rs` 启动**真实**的 axum 服务（`ib::app_router`，即生产路由表）
监听 `127.0.0.1:0` 临时端口，配一个 `tempfile` 里的独立 SQLite 库。
HTTP 客户端是手写的 ~40 行 `TcpStream` 实现，只解析需要的状态码、
头部和 JSON 键——不引入 HTTP 客户端依赖。

这套测试存在的意义是覆盖单元测试到不了的层面：**Cookie 往返、跨用户隔离、
以及完全通过公开 API 驱动的账务闭环**。

| 测试 | 覆盖 |
| --- | --- |
| `health_reports_a_live_database` | `/api/health` 真实探活 |
| `index_is_served_with_no_store` | 页面外壳 `no-store` |
| `protected_endpoints_require_a_session` | 无 Cookie 时交易接口 401 |
| `registration_does_not_issue_a_session` | 注册 201 且**不带** Cookie |
| `duplicate_registration_is_a_conflict` | 重复邮箱 409 |
| `login_rejects_a_wrong_password_and_a_bad_email_alike` | 密码错和邮箱不存在返回同样的 401 |
| `invalid_input_is_rejected_before_touching_the_ledger` | 非法参数 400 且不写账本 |
| `users_only_see_their_own_account` | 用户间数据隔离 |
| `a_user_cannot_cancel_or_fill_another_users_order` | 跨用户撤单/成交被拒 |
| `order_fill_and_ledger_stay_consistent` | 订单→成交→持仓/现金/成交明细一致 |
| `an_exec_id_retry_does_not_double_count` | 同 `EXEC_ID` 重试不重复记账 |
| `cancelling_makes_an_order_unfillable` | 撤单后不可再成交 |
| `logout_invalidates_the_session` | 登出后会话失效 |
| `verification_rejects_unknown_and_expired_tokens` | 未知 token 400、过期 token 410 |
| `resend_does_not_reveal_whether_an_email_exists` | 重发对所有邮箱返回同一个 `ok` |
| `a_signed_in_user_always_gets_a_stable_simulation_account` | 同一用户恒得同一模拟账户 |
| `a_duplicate_contract_is_a_conflict` | 重复合约 409 |
| `order_ids_increment_per_account` | 订单号按账户递增 |
| `expired_sessions_are_reclaimed_on_login` | 登录时清理过期会话 |
| `a_cash_deposit_must_be_positive` | 负数与零金额入金返回 400，且不改动余额 |

**未覆盖的区域**（诚实记录）：

- Resend 的真实投递。测试不设 `RESEND_API_KEY`，验证的是**门禁降级**路径；
  真实发信只在人工冒烟时验证。
- 多请求并发。当前 `Pool` 串行化了一切，顺序执行已足够；
  引入真实并发后才需要竞态测试。
- 前端与后端的集成。前端测试全部用 mock 数据，不打真实服务。

### 前端测试（`npm run test`，50 个）

Vitest + jsdom + Testing Library，配置在 `frontend/vite.config.ts`。
`pool: 'vmThreads'` 让每个 worker 复用一个 jsdom 实例——原本每个测试文件
新建一个，开销占了整个运行时间的大头；测试文件级的隔离仍然保留。

| 文件 | 数量 | 覆盖 |
| --- | --- | --- |
| `useTrading.test.tsx` | 16 | `summarize` 汇总（挂单/成交计数、基础币种现金、持仓市值、最近成交、逐合约聚合）、状态筛选分页、合约搜索、默认合约预选且不覆盖用户选择、非正数入金拒绝、重复合约前端拦截、LMT 必填限价、加载失败标记为 stale、401 自动登出 |
| `pages.test.tsx` | 14 | 持仓符号回退到 conid、空态行、成交列表、订单页仅对可提交订单显示撤单/成交按钮、进行中按钮文案、展开/收起详情、筛选切页重置、单页时隐藏分页器、边界禁用 |
| `contract.test.ts` | 5 | `api-schema.json` 的条目集合、类型声明语法、可空字段、交叉引用是否都能解析；`_ContractFieldsMatch` 断言仍然存在 |
| `api.test.ts` | 6 | 路径前缀与 `same-origin` 凭据、成功解析、`ApiError` 携带状态码与消息、非 JSON 响应回退通用文案、保留调用方 `init`、POST 构造 |
| `types.test.ts` | 5 | hash 路由解析、未知/空 hash 回退 `overview`、形似但非法的 hash 被忽略、每个视图都有标签与标题、标签不重复 |
| `components.test.tsx` | 4 | `PanelTitle`、`DataTable`（含可选操作区）、`Empty` 跨列、`Metric` 色调 |

## 尚未覆盖

- 无 `cargo bench` / 性能回归基线。
- 无属性测试（property-based testing）；定点换算和切分器这类逻辑很适合。
- 无覆盖率报告。
- `api-schema.json` 的值类型（`"string|null"` 等）是给人看的，TypeScript 会把
  导入的 JSON 字符串放宽成 `string`，无法据此推导类型。`types.ts` 因此手写值类型，
  只在编译期断言**键集**一致；值的正确性靠 Rust 侧测试和上面那个前端测试兜底。
- `Overview` 的契约没有 Rust 单元测试直接覆盖（它在 `src/trading.rs` 里，
  字段私有），只由前端断言和读真实端点的集成测试间接覆盖。
- 端到端测试是手写 HTTP 客户端，不支持 HTTP/1.1 chunked 响应。
  当前所有端点都用定长响应，暂时够用。
