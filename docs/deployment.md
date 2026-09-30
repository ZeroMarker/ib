# 配置与部署

## 环境变量

| 变量 | 必填 | 默认 | 说明 |
| --- | --- | --- | --- |
| `DB_PATH` | 否 | `./ib.sqlite3` | SQLite 文件路径。父目录不存在会自动创建 |
| `SERVER_ADDR` | 否 | `127.0.0.1:8081` | `ib serve` 监听地址。可被位置参数 `ib serve ADDR` 覆盖 |
| `RESEND_API_KEY` | 否 | 无 | Resend API Key。**空或未设置 = 关闭邮箱验证功能** |
| `RESEND_FROM` | 否 | `ib <onboarding@resend.dev>` | 发件人身份。默认值只投递给 Resend 沙箱地址 |
| `APP_BASE_URL` | 否 | `http://127.0.0.1:8081` | 验证链接的公开来源前缀，**不带结尾斜杠** |
| `RUST_BACKTRACE` | 否 | 无 | 仅在 `deploy/ib.service` 中设为 `1` |

### `RESEND_API_KEY` 未配置时的行为

这是**降级而非报错**的开关，全套行为都经过设计：

| 行为 | 已配置 | 未配置 |
| --- | --- | --- |
| 注册 | 建账户 + 发信，201 | 建账户，201 |
| 登录时邮箱未验证 | **403 拒绝** | **200 放行**（防本地环境把自己锁死） |
| 重发验证邮件 | 实际发送 | 跳过，返回同一个 `ok` |
| `email_verified` 字段 | 验证后为 `true` | **恒为 `false`，绝不伪造已验证** |
| 发信失败 | 记日志，账户保持未验证 | — |

「登录门禁降级」是刻意的：配了 Resend 但投递失败时，如果无差别拒绝登录，
用户会被永久锁在外面。门禁只依赖「本服务是否具备发信能力」这一个信号。

`APP_BASE_URL` 和 `RESEND_FROM` 在没有 API Key 时不会被读取。

## 本地运行

```bash
# 1. 构建（顺序不能颠倒，见架构文档）
cd frontend && npm ci && npm run build && cd ..
cargo build --release

# 2. 建表
export DB_PATH=./ib.sqlite3
./target/release/ib init-db

# 3. 起服务
./target/release/ib serve
# simulation auth API listening on http://127.0.0.1:8081
```

打开 <http://localhost:8081>，注册一个账户即可。

> 用 `localhost` 而不是 `127.0.0.1` 或局域网 IP：会话 Cookie 带 `Secure` 标记，
> 浏览器在明文 HTTP 下只对 localhost 放行。详见 [API 文档](api.md#会话-cookie)。

`ib serve` 需要**已存在的表**。忘记 `init-db` 时接口会因缺表而报错，
用 `ib ping` 只能验证连接、不能验证 schema。

## 前端开发

`vite.config.ts` 配了 `server.proxy`，把 `/api` 转发给 `ib serve`，
所以 `npm run dev` 可以直接联调，不需要先构建前端。

```bash
# 终端 1：后端，提供 API
./target/release/ib serve                      # 默认 127.0.0.1:8081

# 终端 2：前端开发服务器，带 HMR
cd frontend
npm run dev                                   # http://localhost:5173
```

后端换了地址时用 `IB_API_TARGET` 告诉代理：

```bash
IB_API_TARGET=http://127.0.0.1:9090 npm run dev
```

其他前端命令：

```bash
npm run check    # format:check + lint + test
npm run build    # tsc -b && vite build（产物进 dist/，需重建后端才生效）
```

### 缓存版本

Service Worker 缓存和静态资源版本串的**单一来源是 `frontend/version.json`**：

| 键 | 作用 |
| --- | --- |
| `CACHE_NAME` | 缓存分区名。改动被缓存的页面壳时递增，旧的会在 `activate` 时清掉 |
| `ASSET_VERSION` | `?v=` 查询串。新构建后递增 |

两者如何到达最终产物：

- `ASSET_VERSION` 由 `src/App.tsx` 和 `vite.config.ts` 从 JSON 读入，
  分别用于 SW 注册 URL 和 `index.html` 里的资源查询串。
- `public/sw.js` 里的 `__ASSET_VERSION__` 占位符由 `versioned-service-worker`
  插件在 `closeBundle` 注入（Vite 原样复制 `public/`，不会解析其 import）。
  **占位符不存在时构建直接失败**——否则会把 `?v=__ASSET_VERSION__` 发到线上，
  所有构建共用一个缓存键，缓存永远不失效。

`CACHE_NAME` 只在 `public/sw.js` 里出现一次，改缓存行为时才需要动它。

## Caddy 反向代理

`deploy/Caddyfile` 把 `https://ibkr.20070809.xyz/` 代理到本机 `127.0.0.1:8081`，
并开启 gzip：

```
ibkr.20070809.xyz {
	encode gzip

	reverse_proxy 127.0.0.1:8081
}
```

```bash
caddy validate --config deploy/Caddyfile
sudo caddy reload --config deploy/Caddyfile
```

线上走 HTTPS，因此 `Secure` Cookie 正常工作。Caddy 在前面也意味着
`APP_BASE_URL` 必须设为对外域名，否则验证邮件里的链接会指向 `127.0.0.1`——
用户点开是打不开的。

## systemd 部署

`deploy/ib.service` 以 `ubuntu` 用户运行 release 二进制。

### 安装

```bash
sudo install -d -m 0750 /etc/ib
sudo install -o root -g ubuntu -m 0640 deploy/ib.env.example /etc/ib/ib.env
sudoedit /etc/ib/ib.env        # 填入 RESEND_API_KEY、APP_BASE_URL 等

sudo install -m 0644 deploy/ib.service /etc/systemd/system/ib.service
sudo systemctl daemon-reload
sudo systemctl enable --now ib.service
sudo systemctl status ib.service
```

### 单元要点

| 设置 | 值 | 作用 |
| --- | --- | --- |
| `User` / `Group` | `ubuntu` | 不以 root 运行 |
| `WorkingDirectory` | `/home/ubuntu/ib` | `ib.sqlite3` 相对路径的基准（`EnvironmentFile` 已覆盖为绝对路径） |
| `EnvironmentFile` | `/etc/ib/ib.env` | 权限 `0640 root:ubuntu` |
| `StateDirectory` | `ib` | 自动创建 `/var/lib/ib`，并设为 `DB_PATH` |
| `ProtectHome` | `read-only` | 只读挂载 `/home`；二进制在 `/home` 下仍可执行，`/var/lib` 仍可写 |
| `ProtectSystem` | `full` | `/usr` 和 `/boot` 只读 |
| `PrivateTmp` / `NoNewPrivileges` | `true` | 基本沙箱 |
| `UMask` | `027` | 新建文件组内可读、其他用户不可见 |
| `Restart` | `on-failure`，间隔 5 秒 | 崩溃重启；正常 SIGTERM 退出不会触发重启 |
| `LimitNOFILE` | 65536 | |

数据库因此位于 `/var/lib/ib/ib.sqlite3`，由 `StateDirectory` 保证在
`ProtectHome=read-only` 下可写。

### 运维

```bash
journalctl -u ib.service -f          # 实时日志
journalctl -u ib.service -p err      # 只看错误
sudo systemctl restart ib.service    # 重启（发 SIGTERM，走优雅排空）
```

`restart` / `stop` 触发 SIGTERM，服务会先排空连接再退出，不会丢弃进行中的账本写入。

### 升级

```bash
cd /home/ubuntu/ib
git pull
cd frontend && npm ci && npm run build && cd ..
cargo build --release
sudo systemctl restart ib.service
```

二进制替换即生效——前端产物已嵌入其中。**新迁移不会自动执行**，
若版本引入了新迁移，需要手动跑一次：

```bash
sudo -u ubuntu DB_PATH=/var/lib/ib/ib.sqlite3 ./target/release/ib init-db
```

`init-db` 幂等，跑第二次无副作用。注意这一步需要在 `ib.service` 停止时执行，
避免与运行中的服务争用写锁。

## 数据目录

| 路径 | 内容 | 备份 |
| --- | --- | --- |
| `/var/lib/ib/` | SQLite 数据库 + `-wal` / `-shm` | **需停服备份**，WAL 模式下直接 `cp` 不安全 |
| `/etc/ib/ib.env` | 密钥与配置，含 `RESEND_API_KEY` | 不应进版本库 |
| `/home/ubuntu/ib/target/release/ib` | 二进制 | 由重新构建产生 |
