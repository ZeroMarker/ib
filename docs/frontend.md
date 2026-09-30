# 前端

Vite + React 18 + TypeScript 5。**没有路由库，没有状态管理库，没有 UI 框架**——
视图切换用 `location.hash`，数据状态用一个自定义 hook，样式是一份手写 CSS。

构建产物进 `frontend/dist/`，被 `src/web.rs` 用 `include_str!` 嵌入 Rust 二进制。

## 目录结构

```
frontend/src/
├── main.tsx                  挂载点（10 行）
├── App.tsx                   启动：注册 SW、捕获安装提示、恢复登录态
├── api.ts                    fetch 封装 + ApiError
├── types.ts                  领域类型 + 视图元数据 + hash 路由解析
├── hooks/useTrading.ts       工作台全部状态与操作
├── components/
│   ├── Layout.tsx            导航与页面骨架（9 个导出组件）
│   └── index.tsx             通用展示件（PanelTitle/DataTable/Empty/Metric）
├── pages/
│   ├── Dashboard.tsx         唯一调用 useTrading 的地方，负责装配
│   ├── OverviewPage.tsx      总览
│   ├── TradePage.tsx         交易终端（合约/订单/现金三张表单）
│   ├── OrdersPage.tsx        订单管理（筛选/分页/展开/撤单/成交）
│   ├── PositionsPage.tsx     持仓
│   └── FillsPage.tsx         成交记录
├── styles.css                全部样式
└── test/                     6 个测试文件，50 个用例
```

## 视图与路由

`types.ts` 定义 5 个视图，每个带 `{ label, title, eyebrow }` 元数据：

| 视图 | hash | 标签 | 用途 |
| --- | --- | --- | --- |
| `overview` | `#overview` | 总览 | 会话快照、指标卡、合约观察 |
| `trade` | `#trade` | 交易终端 | 添加合约、下单、设置现金 |
| `orders` | `#orders` | 订单管理 | 筛选、分页、展开详情、撤单、注资成交 |
| `positions` | `#positions` | 持仓 | 多空持仓与平均成本 |
| `fills` | `#fills` | 成交记录 | 成交明细 |

`viewFromHash()` 对非法或空 hash 一律回退 `overview`。
首次挂载时若 URL 没有 hash，`replaceState` 补上 `#overview`。

切换视图会：平滑滚动到对应区块、把 `document.title` 设为 `<标题> · ib paper`。

用 hash 而非 history API 的原因：静态外壳只有一个 `index.html`，
hash 路由不需要服务端为任意路径回退到 `index.html`（后端确实有
`/login`、`/register` 路由回退外壳，但前端不用它们）。

## 状态管理

`useTrading(onLogout)` 持有工作台的**全部**状态：账本数据、三个表单、
订单筛选/分页/展开、成交弹窗、加载与错误状态、快捷动作锁。返回一个大对象
（40+ 字段）直接分发给页面组件。

页面组件是纯展示 + 回调，不各自持有账本数据。这样避免了 props 钻取
（`Dashboard` 一次性把需要的东西都传下去）。

### 关键约定

**`onLogout` 必须稳定。** `useTrading` 在 `onLogout` 身份变化时会重新执行
加载 effect。`App.tsx` 用 `useCallback(() => setUser(null), [])` 固定它，
否则每次 `App` 渲染都会触发一次 overview 请求。

**并发刷新防乱序。** `loadSeq` ref 记录自增序号，每次 `load` 取号，
响应回来后先比对序号，被后发请求取代的旧响应直接丢弃、不写 state。

**派生数据用 `useMemo`。** `summarize(overview)` 把总览压成 8 个派生值
（`openOrders`、`filledOrders`、`filledQuantity`、`cashTotal`、`positionCost`、
`latestFill`、`contractActivity`、`symbols`），只在 `overview` 变化时算一次。
早先的实现是每次渲染重算 `contractActivity` 的 O(合约×订单) 循环。

> `contractActivity` 早期是 O(合约 × 订单 + 持仓) 的每渲染重算。
> 现在改成先建 `conid → 订单数` 的 Map，再单次遍历合约组装。

**空集合用模块级常量。** `EMPTY_SYMBOLS` / `EMPTY_ACTIVITY` 保证
「数据未加载」时返回的引用稳定，不让下游 `useMemo`/`useEffect` 反复触发。

**默认合约预选写成条件式 setState。** 合约加载后需要给下单表单一个默认
`conid`，但在渲染期间推导会跟用户输入打架（每次按键都被重置）。
做法是在数据变化的位置做一次条件更新，由 `defaultedConid` state 保证只发生一次。

**快捷键只订阅一次。** 监听器读 `activeViewRef` / `busyActionRef` 拿当前值，
ref 在 effect 里同步而不是在渲染期间写，避免重新订阅。

**录入校验。** 表单提交前在前端校验一遍（正数数量、LMT 必填限价、
STP 必填触发价、重复合约、非法币种），`busyAction` 非空时拒绝新的动作，
既防重复提交也防并发。

## 键盘快捷键

在 `useTrading` 里全局监听 `keydown`：

| 键 | 行为 |
| --- | --- |
| `1` – `5` | 切换到第 N 个视图 |
| `R` / `r` | 刷新账本 |
| `Esc` | 关闭成交弹窗 |

以下情况不响应：按住 `Meta`/`Ctrl`/`Alt`；焦点在 `INPUT` / `SELECT` /
`TEXTAREA` / `contentEditable` 内。

## API 客户端

`api.ts` 全部内容（20 行）：

```ts
export const api = async <T>(path: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(`api/${path}`, { credentials: 'same-origin', ...init })
  const data = await response.json().catch(() => undefined)
  if (!response.ok) throw new ApiError(response.status, data?.error ?? '请求失败，请稍后重试。')
  return data as T
}
```

要点：

- **相对路径 `api/...`**（无前导斜杠），配合 `base: './'` 支持子路径部署。
- **`credentials: 'same-origin'`** 让会话 Cookie 随请求发送。
- 响应体不是 JSON 时（代理错误页等）回退到通用中文文案，不抛解析异常。
- `ApiError` 携带 `status`，调用方靠它分流：401 触发登出，其他显示 `message`。

`json()` 辅助函数构造 `Content-Type: application/json` 的 POST。

### 类型契约

`api-schema.json`（仓库根目录）是前后端共享的**线上契约**：

- `src/models.rs` 的单元测试断言序列化出的字段与该文件逐项一致，
  所以文件不会和服务端实现漂移。
- `types.ts` 在**编译期**断言自己的类型键集与该文件一致
  （`_ContractFieldsMatch`）。服务端加字段而前端没加，`tsc` 立刻失败。
- `src/test/contract.test.ts` 覆盖类型断言管不到的部分：文件本身是否合法、
  类型声明语法、哪些字段可空、交叉引用的名字是否存在，以及断言本身没被删掉。

值类型（`string | null` 等）在 `types.ts` 里是手写的：TypeScript 会把导入的
JSON 字符串放宽成 `string`，无法从 `"string|null"` 推导。**键集**才是编译期断言的对象。

这个机制存在的原因是一次真实事故：`Position`、`Cash`、`Fill` 曾长期是服务端
字段的严格子集，页面只读恰好被手工打上类型的那几个字段，服务端新增的字段
既不报错也不可用。

## 邮箱验证流程

三处 UI 协作完成验证：

1. **URL 参数自动验证。** `App.tsx` 启动时读 `?verify_token=`，
   调用 `POST auth/verify`，然后立刻用 `replaceState` 把 token 从 URL 上抹掉
   （避免刷新时重复消费一次性 token）。
2. **验证横幅。** `Layout.tsx` 的 `VerifyNote` 在已登录但未验证时显示，
   支持「重发」和「粘贴验证码」两条路径。
3. **登录 403 降级引导。** 登录被 403 拒绝时，`Auth` 组件把消息换成
   「邮箱尚未验证」并显示「重发验证邮件」按钮。

## PWA

`public/manifest.webmanifest` + 192/512 图标 + `public/sw.js`。
`App.tsx` 捕获 `beforeinstallprompt`，`Dashboard` 在侧栏提供安装入口。

Service Worker 只缓存页面壳，`fetch` 事件**显式跳过 `/api/` 路径**：

```js
if (request.method !== 'GET' || new URL(request.url).pathname.includes('/api/')) return
```

导航请求走 network-first 并回退到缓存的 `./`；静态资源走 network-first
并写入缓存。因此离线时能看到界面，但读不到账本——交易数据永不过期。

### 缓存版本

版本串的单一来源是 `frontend/version.json`，不再有需要人工同步的 4 处。
机制细节见[部署文档](deployment.md#缓存版本)。

## 样式

单份 `styles.css`，无 CSS 框架、无 CSS 变量主题层、无预处理器。
类名走 BEM 风格的连字符命名（`.app-sidebar`、`.sidebar-link.active`、
`.metric-card.metric-warn`）。

响应式靠媒体查询：桌面侧栏 → 移动端底部导航（`MobileNav`），
并做了安全区域适配（`env(safe-area-inset-*)`）。

无内联 `style` 属性——动态样式一律走 CSS 类（早期有过内联 style，
已改为 `.verify-token-form` / `.verify-message` 这类类名）。

## 已知限制

- 订单分页是**客户端**分页，每页 8 条，筛选也在客户端做。
  订单量大了需要改成服务端分页（`GET /api/trading/orders` 目前不支持过滤参数）。
- `contractActivity` 等派生值仍在客户端计算，合约和订单都上千时会成为瓶颈。
- 无错误边界（Error Boundary）：渲染期异常会导致整页白屏。
- 无深色模式、无浏览器通知、无多模拟账户切换（见 [TODO](../TODO.md)）。
