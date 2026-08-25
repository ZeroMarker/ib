import { Fragment, StrictMode, type FormEvent, type ReactNode, useEffect, useMemo, useState } from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'

type AuthUser = { user_id: string; email: string; email_verified: boolean }
type Contract = { conid: number; symbol: string; sec_type: string; exchange: string; currency: string }
type Order = { order_id: number; account_id: string; conid: number; side: string; order_type: string; total_quantity: string; filled_quantity: string; status: string; lmt_price: string | null; aux_price: string | null }
type Position = { conid: number; position: string; avg_cost: string | null }
type Cash = { currency: string; cash: string }
type Fill = { exec_id: string; order_id: number; quantity: string; price: string }
type Overview = { account: { account_id: string; account_type: string; currency: string; status: string }; contracts: Contract[]; orders: Order[]; positions: Position[]; cash: Cash[]; fills: Fill[] }
type InstallPrompt = Event & { prompt: () => Promise<void>; userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }> }
type View = 'overview' | 'trade' | 'orders' | 'positions' | 'fills'

const viewMeta: Record<View, { label: string; title: string; eyebrow: string }> = {
  overview: { label: '总览', title: '交易工作台', eyebrow: 'SIMULATION / OVERVIEW' },
  trade: { label: '交易终端', title: '提交模拟订单', eyebrow: 'SIMULATION / ORDER TICKET' },
  orders: { label: '订单管理', title: '订单管理', eyebrow: 'SIMULATION / ORDERS' },
  positions: { label: '持仓账户', title: '持仓账户', eyebrow: 'SIMULATION / POSITIONS' },
  fills: { label: '成交记录', title: '成交记录', eyebrow: 'SIMULATION / FILLS' },
}

const viewFromHash = (): View => {
  const value = window.location.hash.slice(1) as View
  return value in viewMeta ? value : 'overview'
}

const api = async <T,>(path: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(`api/${path}`, { credentials: 'same-origin', ...init })
  const data = await response.json().catch(() => undefined)
  if (!response.ok) throw new Error(data?.error ?? '请求失败，请稍后重试。')
  return data as T
}

const json = (body: unknown): RequestInit => ({ method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })

function App() {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [booting, setBooting] = useState(true)
  const [installPrompt, setInstallPrompt] = useState<InstallPrompt | null>(null)

  useEffect(() => {
    if ('serviceWorker' in navigator) navigator.serviceWorker.register('./sw.js?v=20260825-ux4').catch(() => {})
    const captureInstall = (event: Event) => { event.preventDefault(); setInstallPrompt(event as InstallPrompt) }
    window.addEventListener('beforeinstallprompt', captureInstall)
    api<AuthUser>('auth/me').then(setUser).catch(() => {}).finally(() => setBooting(false))
    return () => window.removeEventListener('beforeinstallprompt', captureInstall)
  }, [])

  if (booting) return <div className="loading-screen">正在载入模拟交易空间…</div>
  return user
    ? <Dashboard user={user} onLogout={() => setUser(null)} installPrompt={installPrompt} onInstalled={() => setInstallPrompt(null)} />
    : <Auth onAuthenticated={setUser} />
}

function Auth({ onAuthenticated }: { onAuthenticated: (user: AuthUser) => void }) {
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [message, setMessage] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    setMessage('')
    setSubmitting(true)
    try {
      onAuthenticated(await api<AuthUser>(`auth/${mode}`, json({ email, password })))
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '请求失败')
    } finally {
      setSubmitting(false)
    }
  }

  return <main className="shell auth-shell">
    <section className="hero">
      <div className="brand-mark">ib</div>
      <p className="eyebrow">PAPER TRADING PLATFORM</p>
      <h1>把策略想法，<br /><em>安全地跑一遍。</em></h1>
      <p className="hero-copy">用于策略开发、纸上交易和账务演练的模拟交易平台。</p>
      <div className="hero-points"><span>实时账本</span><span>多空持仓</span><span>不触碰真实市场</span></div>
    </section>
    <section className="auth-card" aria-label="用户认证">
      <div className="tabs"><button className={mode === 'login' ? 'tab active' : 'tab'} onClick={() => setMode('login')}>登录</button><button className={mode === 'register' ? 'tab active' : 'tab'} onClick={() => setMode('register')}>注册</button></div>
      <div className="card-heading"><p className="eyebrow">{mode === 'login' ? 'WELCOME BACK' : 'START SIMULATING'}</p><h2>{mode === 'login' ? '登录账户' : '创建账户'}</h2><p>{mode === 'login' ? '进入你的模拟交易空间。' : '开始你的纸上交易旅程。'}</p></div>
      <form onSubmit={submit}>
        <label htmlFor="email">邮箱</label><input id="email" type="email" autoComplete="email" placeholder="you@example.com" value={email} onChange={(event) => setEmail(event.target.value)} required />
        <label htmlFor="password">密码</label><input id="password" type="password" autoComplete={mode === 'login' ? 'current-password' : 'new-password'} placeholder="至少 8 位" minLength={8} value={password} onChange={(event) => setPassword(event.target.value)} required />
        <button className="primary-button" disabled={submitting}>{submitting ? '处理中…' : mode === 'login' ? '登录' : '注册'}</button>
        <p className="form-message">{message}</p>
      </form>
      <p className="legal">继续即表示你了解这是模拟交易服务，不会发送真实订单。</p>
    </section>
  </main>
}

function Dashboard({ user, onLogout, installPrompt, onInstalled }: { user: AuthUser; onLogout: () => void; installPrompt: InstallPrompt | null; onInstalled: () => void }) {
  const [activeView, setActiveView] = useState<View>(viewFromHash)
  const [overview, setOverview] = useState<Overview | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [contractForm, setContractForm] = useState({ conid: '', symbol: '', exchange: 'SMART', currency: 'USD' })
  const [orderForm, setOrderForm] = useState({ conid: '', side: 'BUY', order_type: 'MKT', quantity: '100', price: '', stopPrice: '' })
  const [cashForm, setCashForm] = useState({ currency: 'USD', amount: '100000' })
  const [fillTarget, setFillTarget] = useState<number | null>(null)
  const [fillPrice, setFillPrice] = useState('')
  const [contractQuery, setContractQuery] = useState('')
  const [orderFilter, setOrderFilter] = useState('ALL')
  const [orderPage, setOrderPage] = useState(0)
  const [expandedOrder, setExpandedOrder] = useState<number | null>(null)
  const [busyAction, setBusyAction] = useState('')
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null)

  useEffect(() => {
    const syncView = () => setActiveView(viewFromHash())
    window.addEventListener('hashchange', syncView)
    if (!window.location.hash) window.history.replaceState(null, '', '#overview')
    return () => window.removeEventListener('hashchange', syncView)
  }, [])
  const load = async () => {
    try { setOverview(await api<Overview>('trading/overview')); setError(''); setLastUpdated(new Date()) }
    catch (reason) { setError(reason instanceof Error ? reason.message : '无法读取交易数据') }
  }
  useEffect(() => { load() }, [])
  useEffect(() => {
    if (overview?.contracts.length && !overview.contracts.some((item) => String(item.conid) === orderForm.conid)) {
      setOrderForm((form) => ({ ...form, conid: String(overview.contracts[0].conid) }))
    }
  }, [overview])

  const symbols = useMemo(() => new Map((overview?.contracts ?? []).map((item) => [item.conid, item.symbol])), [overview])
  const visibleContracts = overview?.contracts.filter((item) => `${item.symbol} ${item.conid} ${item.exchange}`.toLowerCase().includes(contractQuery.toLowerCase())) ?? []
  const filteredOrders = overview?.orders.filter((order) => orderFilter === 'ALL' || (orderFilter === 'OPEN' ? ['Submitted', 'PreSubmitted'].includes(order.status) : order.status.toUpperCase() === orderFilter)) ?? []
  const pageSize = 8
  const pageCount = Math.max(1, Math.ceil(filteredOrders.length / pageSize))
  const safeOrderPage = Math.min(orderPage, pageCount - 1)
  const pageOrders = filteredOrders.slice(safeOrderPage * pageSize, (safeOrderPage + 1) * pageSize)
  const openOrders = overview?.orders.filter((order) => ['Submitted', 'PreSubmitted'].includes(order.status)).length ?? 0
  const filledOrders = overview?.orders.filter((order) => order.status.toUpperCase() === 'FILLED').length ?? 0
  const filledQuantity = overview?.orders.reduce((total, order) => total + Number(order.filled_quantity), 0) ?? 0
  const cashTotal = overview?.cash.reduce((total, item) => total + Number(item.cash), 0) ?? 0
  const positionCost = overview?.positions.reduce((total, item) => total + Math.abs(Number(item.position)) * Number(item.avg_cost ?? 0), 0) ?? 0
  const latestFill = overview?.fills[0]
  const contractActivity = (overview?.contracts ?? []).map((contract) => ({
    ...contract,
    orders: overview?.orders.filter((order) => order.conid === contract.conid).length ?? 0,
    position: overview?.positions.find((item) => item.conid === contract.conid)?.position ?? '0',
  }))
  const action = async (work: () => Promise<void>, success: string, actionName = 'action') => {
    if (busyAction) return
    setBusyAction(actionName)
    try { await work(); setNotice(success); setError(''); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : '操作失败') }
    finally { setBusyAction('') }
  }
  const addContract = (event: FormEvent) => {
    event.preventDefault()
    const conid = Number(contractForm.conid)
    const symbol = contractForm.symbol.trim().toUpperCase()
    if (!Number.isInteger(conid) || conid <= 0 || !symbol) return setError('请填写有效的合约 ConID 和代码。')
    if (overview?.contracts.some((item) => item.conid === conid || item.symbol.toUpperCase() === symbol)) return setError('该合约已存在，请直接从下方选择。')
    return action(async () => { await api('trading/contracts', json({ ...contractForm, conid, symbol, sec_type: 'STK' })); setContractForm({ conid: '', symbol: '', exchange: 'SMART', currency: 'USD' }) }, '合约已添加。', 'contract')
  }
  const placeOrder = (event: FormEvent) => {
    event.preventDefault()
    const quantity = Number(orderForm.quantity)
    const needsLimit = ['LMT', 'STP_LMT'].includes(orderForm.order_type)
    const needsStop = ['STP', 'STP_LMT'].includes(orderForm.order_type)
    const price = Number(orderForm.price)
    const stopPrice = Number(orderForm.stopPrice)
    if (!orderForm.conid || !Number.isFinite(quantity) || quantity <= 0) return setError('请选择合约并填写有效数量。')
    if (needsLimit && (!Number.isFinite(price) || price <= 0)) return setError('该订单必须填写有效限价。')
    if (needsStop && (!Number.isFinite(stopPrice) || stopPrice <= 0)) return setError('该订单必须填写有效触发价。')
    return action(async () => {
      await api('trading/orders', json({
        conid: Number(orderForm.conid), side: orderForm.side, order_type: orderForm.order_type,
        quantity: orderForm.quantity, lmt_price: needsLimit ? orderForm.price : null, aux_price: needsStop ? orderForm.stopPrice : null,
      }))
    }, '订单已提交。', 'order')
  }
  const setCash = (event: FormEvent) => { event.preventDefault(); const amount = Number(cashForm.amount); if (!/^[a-zA-Z]{3}$/.test(cashForm.currency) || !Number.isFinite(amount)) return setError('请填写有效的币种和金额。'); return action(async () => { await api('trading/cash', json({ ...cashForm, currency: cashForm.currency.toUpperCase() })) }, '现金余额已更新。', 'cash') }
  const cancel = (orderId: number) => action(async () => { await api(`trading/orders/${orderId}/cancel`, { method: 'POST' }) }, '订单已撤销。', `cancel-${orderId}`)
  const submitFill = (event: FormEvent) => { event.preventDefault(); const price = Number(fillPrice); if (fillTarget === null || !Number.isFinite(price) || price <= 0) return setError('请输入有效的成交价格。'); return action(async () => { await api(`trading/orders/${fillTarget}/fill`, json({ price: fillPrice })); setFillTarget(null); setFillPrice('') }, '模拟成交已记账。', `fill-${fillTarget}`) }
  const install = async () => { if (!installPrompt) return; await installPrompt.prompt(); await installPrompt.userChoice; onInstalled() }
  const logout = async () => { await api('auth/logout', { method: 'POST' }).catch(() => {}); onLogout() }
  const currentView = viewMeta[activeView]

  return <main className="dashboard-page">
    <aside className="app-sidebar"><div className="sidebar-brand"><div className="brand-mark small">ib</div><div><strong>ib paper</strong><span>SIMULATION OS</span></div></div><div className="sidebar-section"><p>工作区</p><a className={activeView === 'overview' ? 'sidebar-link active' : 'sidebar-link'} href="#overview"><span>⌂</span>总览</a><a className={activeView === 'trade' ? 'sidebar-link active' : 'sidebar-link'} href="#trade"><span>⌁</span>交易终端</a><a className={activeView === 'orders' ? 'sidebar-link active' : 'sidebar-link'} href="#orders"><span>≡</span>订单管理 <b>{openOrders}</b></a><a className={activeView === 'positions' ? 'sidebar-link active' : 'sidebar-link'} href="#positions"><span>◫</span>持仓账户</a><a className={activeView === 'fills' ? 'sidebar-link active' : 'sidebar-link'} href="#fills"><span>◌</span>成交记录</a></div><div className="sidebar-section"><p>工具</p><button className="sidebar-link sidebar-button" onClick={() => action(load, '数据已刷新。', 'refresh')} disabled={busyAction !== ''}><span>↻</span>同步数据</button><button className="sidebar-link sidebar-button" onClick={install} disabled={!installPrompt}><span>⇩</span>安装 PWA</button></div><div className="sidebar-footer"><span className="status-dot" />模拟环境运行中<small>Oracle ledger · session safe</small></div></aside>
    <div className="dashboard-content">
    <header className="dashboard-header"><div className="brand-line"><div className="mobile-brand-mark brand-mark small">ib</div><div><p className="eyebrow">{currentView.eyebrow}</p><h1>{currentView.title}</h1></div></div><div className="header-actions">{installPrompt && <button className="install-button" onClick={install}>安装应用</button>}<span className="sync-pill"><span className="status-dot" />数据已同步</span><span className="user-pill"><span className="status-dot" />{user.email}</span><button className="text-button" onClick={logout}>退出登录</button></div></header>
    <nav className="mobile-nav" aria-label="移动端工作台导航"><a className={activeView === 'overview' ? 'active' : ''} href="#overview"><span>⌂</span>总览</a><a className={activeView === 'trade' ? 'active' : ''} href="#trade"><span>⌁</span>交易</a><a className={activeView === 'orders' ? 'active' : ''} href="#orders"><span>≡</span>订单</a><a className={activeView === 'positions' ? 'active' : ''} href="#positions"><span>◫</span>持仓</a><a className={activeView === 'fills' ? 'active' : ''} href="#fills"><span>◌</span>成交</a></nav>
    <div className="verify-note">{user.email_verified ? '邮箱已验证。' : '邮箱验证将在 Resend 邮件服务接入后开放。'}</div>
    {error && <div className="alert error"><span>{error}</span><button className="alert-action" disabled={busyAction !== ''} onClick={() => action(load, '数据已刷新。', 'refresh')}>重试</button></div>}{notice && <div className="alert success">{notice}</div>}
    {!overview ? <div className="loading-card">正在读取账户数据…</div> : <>
      {activeView === 'overview' && <section id="overview" className="account-hero"><div><div className="account-kicker"><span className="live-pulse" />PAPER ACCOUNT <span>·</span> {overview.account.status}</div><h2>准备好测试下一笔交易。</h2><p>这是你的隔离模拟账户，所有订单只会写入模拟账本。</p><div className="account-meta"><span><small>账户编号</small><strong>{overview.account.account_id}</strong></span><span><small>账户类型</small><strong>{overview.account.account_type}</strong></span><span><small>基础币种</small><strong>{overview.account.currency}</strong></span></div></div><div className="account-visual"><div className="ring ring-large"><span>100%</span><small>模拟安全</small></div><span className="visual-caption">REAL MARKET DISCONNECTED</span></div></section>}
      <nav className="workspace-nav" aria-label="工作台导航"><a className={activeView === 'overview' ? 'active' : ''} href="#overview">总览</a><a className={activeView === 'trade' ? 'active' : ''} href="#trade">交易</a><a className={activeView === 'orders' ? 'active' : ''} href="#orders">订单 <b>{openOrders}</b></a><a className={activeView === 'positions' ? 'active' : ''} href="#positions">持仓 <b>{overview.positions.length}</b></a><a className={activeView === 'fills' ? 'active' : ''} href="#fills">成交 <b>{overview.fills.length}</b></a><span className="refresh-meta">{lastUpdated ? `更新于 ${lastUpdated.toLocaleTimeString()}` : '尚未同步'}<button className="text-button" disabled={busyAction !== ''} onClick={() => action(load, '数据已刷新。', 'refresh')}>刷新</button></span></nav>
      {activeView === 'overview' && <>
      <section className="metric-grid"><Metric label="现金余额" value={cashTotal ? cashTotal.toLocaleString('en-US', { maximumFractionDigits: 2 }) : '—'} hint={`${overview.account.currency} · 可用资金`} icon="◈" tone="lime" /><Metric label="持仓成本" value={positionCost ? positionCost.toLocaleString('en-US', { maximumFractionDigits: 2 }) : '—'} hint={`${overview.positions.length} 个标的`} icon="◒" tone="blue" /><Metric label="开放订单" value={String(openOrders)} hint="等待成交" icon="◎" tone="amber" /><Metric label="累计成交" value={String(filledQuantity)} hint={`${filledOrders} 笔已完成`} icon="✓" tone="violet" /></section>
      <section className="insight-grid"><div className="insight-card insight-primary"><div className="insight-copy"><p className="eyebrow">SESSION SNAPSHOT</p><h2>你的模拟盘正在等待策略。</h2><p>从订单票据开始，验证价格、数量和账务变化。所有操作都可以撤销或重新演练。</p><div className="insight-actions"><a className="primary-link" href="#trade">开始下单 <span>→</span></a><a className="quiet-link" href="#orders">查看订单</a></div></div><div className="mini-chart" aria-label="模拟账户活动趋势"><span /><span /><span /><span /><span /><span /><span /><span /><i /></div></div><div className="status-card"><div className="card-label"><span>系统状态</span><span className="status-chip">NORMAL</span></div><div className="status-row"><span>API 服务</span><strong><i className="status-dot" />Online</strong></div><div className="status-row"><span>Oracle 账本</span><strong><i className="status-dot" />Connected</strong></div><div className="status-row"><span>撮合模式</span><strong>手动成交</strong></div><div className="status-row"><span>最近成交</span><strong>{latestFill ? `#${latestFill.order_id}` : '暂无'}</strong></div></div></section>
      </>}
      {activeView === 'trade' && <div className="view-intro"><span className="view-number">01</span><div><p className="eyebrow">ORDER TICKET</p><h2>把策略想法转成一笔模拟订单</h2><p>选择合约、方向和订单类型，提交后可在订单管理中追踪状态。</p></div><a className="quiet-link" href="#orders">查看订单 →</a></div>}
      {activeView === 'trade' && <div id="trade" className="trade-layout">
        <section className="trade-panel"><PanelTitle eyebrow="PAPER ORDER" title="提交模拟订单" />
          <form className="compact-form" onSubmit={addContract}><label>新增合约</label><div className="field-row"><input inputMode="numeric" placeholder="ConID" value={contractForm.conid} onChange={(event) => setContractForm({ ...contractForm, conid: event.target.value })} required /><input placeholder="AAPL" value={contractForm.symbol} onChange={(event) => setContractForm({ ...contractForm, symbol: event.target.value })} required /></div><div className="field-row"><input value={contractForm.exchange} onChange={(event) => setContractForm({ ...contractForm, exchange: event.target.value })} placeholder="交易所" /><input value={contractForm.currency} maxLength={3} onChange={(event) => setContractForm({ ...contractForm, currency: event.target.value })} placeholder="币种" /></div><button className="secondary-button" disabled={busyAction !== ''}>{busyAction === 'contract' ? '处理中…' : '添加合约'}</button></form>
          <form className="compact-form" onSubmit={placeOrder}><label>搜索并选择合约</label><input className="search-input" placeholder="代码 / ConID / 交易所" value={contractQuery} onChange={(event) => setContractQuery(event.target.value)} /><select value={orderForm.conid} onChange={(event) => setOrderForm({ ...orderForm, conid: event.target.value })} required><option value="">{visibleContracts.length ? '请选择合约' : '没有匹配的合约'}</option>{visibleContracts.map((item) => <option key={item.conid} value={item.conid}>{item.symbol} · {item.sec_type} · {item.exchange}</option>)}</select><div className="field-row"><div><label>方向</label><select value={orderForm.side} onChange={(event) => setOrderForm({ ...orderForm, side: event.target.value })}><option>BUY</option><option>SELL</option></select></div><div><label>类型</label><select value={orderForm.order_type} onChange={(event) => setOrderForm({ ...orderForm, order_type: event.target.value })}><option>MKT</option><option>LMT</option><option>STP</option><option>STP_LMT</option></select></div></div><div className="field-row"><div><label>数量</label><input inputMode="decimal" value={orderForm.quantity} onChange={(event) => setOrderForm({ ...orderForm, quantity: event.target.value })} required /></div>{orderForm.order_type === 'MKT' ? <div><label>价格</label><input value="市价" disabled /></div> : <div><label>{['STP', 'STP_LMT'].includes(orderForm.order_type) ? '触发价' : '限价'}</label><input inputMode="decimal" placeholder="请输入价格" value={['STP', 'STP_LMT'].includes(orderForm.order_type) ? orderForm.stopPrice : orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, [orderForm.order_type === 'LMT' ? 'price' : 'stopPrice']: event.target.value })} required /></div>}</div>{orderForm.order_type === 'STP_LMT' && <div><label>限价</label><input inputMode="decimal" placeholder="请输入限价" value={orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, price: event.target.value })} required /></div>}<button className="primary-button" disabled={busyAction !== ''}>{busyAction === 'order' ? '提交中…' : '提交订单'}</button></form>
        </section>
        <section className="trade-panel"><PanelTitle eyebrow="CASH LEDGER" title="现金账本" /><form className="compact-form" onSubmit={setCash}><div className="field-row"><div><label>币种</label><input maxLength={3} value={cashForm.currency} onChange={(event) => setCashForm({ ...cashForm, currency: event.target.value })} required /></div><div><label>余额</label><input inputMode="decimal" value={cashForm.amount} onChange={(event) => setCashForm({ ...cashForm, amount: event.target.value })} required /></div></div><button className="secondary-button" disabled={busyAction !== ''}>{busyAction === 'cash' ? '处理中…' : '设置模拟余额'}</button></form><div className="mini-list">{overview.cash.length ? overview.cash.map((item) => <div key={item.currency}><span>{item.currency}</span><strong>{item.cash}</strong></div>) : <span className="muted">暂无余额，可在上方设置模拟余额。</span>}</div></section>
      </div>}
      {activeView === 'overview' && <section className="watchlist-panel"><div className="panel-heading"><PanelTitle eyebrow="INSTRUMENT MONITOR" title="合约观察" /><div className="panel-heading-note"><span className="demo-badge">SIMULATED</span><span>行情接入待配置</span></div></div><div className="watchlist-grid">{contractActivity.length ? contractActivity.slice(0, 6).map((item, index) => <div className="watch-card" key={item.conid}><div className="watch-card-top"><span className={`watch-icon watch-icon-${index % 4}`}>{item.symbol.slice(0, 1)}</span><span className="watch-symbol">{item.symbol}<small>{item.exchange} · {item.currency}</small></span><button className="watch-more" onClick={() => { setContractQuery(item.symbol); window.location.hash = '#trade' }}>•••</button></div><div className="watch-placeholder"><span>行情等待接入</span><strong>—</strong></div><div className="watch-footer"><span>{item.orders} 笔订单</span><span className={Number(item.position) < 0 ? 'negative' : 'positive'}>{Number(item.position) ? `${item.position} 持仓` : '无持仓'}</span></div></div>) : <div className="empty-state"><span>＋</span><strong>添加第一个合约</strong><small>在交易终端中录入合约后，这里会显示监控卡片。</small></div>}</div></section>}
      {activeView === 'orders' && <div id="orders"><DataTable title="订单" eyebrow="ORDERS" action={<button className="text-button" disabled={busyAction !== ''} onClick={() => action(load, '订单已刷新。', 'refresh')}>刷新</button>}><div className="table-toolbar"><div className="filter-tabs">{[['ALL', '全部'], ['OPEN', '开放'], ['FILLED', '已成交'], ['CANCELLED', '已撤销']].map(([value, label]) => <button key={value} className={orderFilter === value ? 'filter-tab active' : 'filter-tab'} onClick={() => { setOrderFilter(value); setOrderPage(0) }}>{label}</button>)}</div><span className="muted">{filteredOrders.length} 条</span></div><table><thead><tr><th>订单</th><th>合约</th><th>方向</th><th>数量</th><th>状态</th><th /></tr></thead><tbody>{pageOrders.length ? pageOrders.map((order) => <Fragment key={`${order.account_id}-${order.order_id}`}><tr><td><button className="row-link" onClick={() => setExpandedOrder(expandedOrder === order.order_id ? null : order.order_id)}>#{order.order_id}</button></td><td>{symbols.get(order.conid) ?? `#${order.conid}`}</td><td><span className={order.side === 'BUY' ? 'side buy' : 'side sell'}>{order.side}</span></td><td>{order.total_quantity}<small className="table-subvalue"> / {order.filled_quantity} 成交</small></td><td><span className={`status status-${order.status.toLowerCase()}`}>{order.status}</span></td><td>{['Submitted', 'PreSubmitted'].includes(order.status) && <><button className="table-action" disabled={busyAction !== ''} onClick={() => cancel(order.order_id)}>{busyAction === `cancel-${order.order_id}` ? '处理中…' : '撤单'}</button><button className="table-action fill-action" disabled={busyAction !== ''} onClick={() => { setFillTarget(order.order_id); setFillPrice(order.lmt_price ?? '') }}>成交</button></>}</td></tr>{expandedOrder === order.order_id && <tr className="detail-row"><td colSpan={6}><div className="order-detail"><span>类型：{order.order_type}</span><span>限价：{order.lmt_price ?? '—'}</span><span>触发价：{order.aux_price ?? '—'}</span><span>成交：{order.filled_quantity} / {order.total_quantity}</span><span>合约 ConID：{order.conid}</span></div></td></tr>}</Fragment>) : <Empty colSpan={6} />}</tbody></table>{pageCount > 1 && <div className="pagination"><button className="table-action" disabled={safeOrderPage === 0} onClick={() => setOrderPage((page) => page - 1)}>上一页</button><span>第 {safeOrderPage + 1} / {pageCount} 页</span><button className="table-action" disabled={safeOrderPage >= pageCount - 1} onClick={() => setOrderPage((page) => page + 1)}>下一页</button></div>}</DataTable></div>}
      {activeView === 'positions' && <div className="single-data-page" id="positions"><DataTable title="持仓" eyebrow="POSITIONS"><table><thead><tr><th>合约</th><th>数量</th><th>平均成本</th></tr></thead><tbody>{overview.positions.length ? overview.positions.map((item) => <tr key={item.conid}><td>{symbols.get(item.conid) ?? `#${item.conid}`}</td><td className={Number(item.position) < 0 ? 'negative' : 'positive'}>{item.position}</td><td>{item.avg_cost ?? '—'}</td></tr>) : <Empty colSpan={3} />}</tbody></table></DataTable></div>}
      {activeView === 'fills' && <div className="single-data-page" id="fills"><DataTable title="成交" eyebrow="FILLS"><table><thead><tr><th>执行 ID</th><th>订单</th><th>数量</th><th>价格</th></tr></thead><tbody>{overview.fills.length ? overview.fills.map((item) => <tr key={item.exec_id}><td>{item.exec_id}</td><td>#{item.order_id}</td><td>{item.quantity}</td><td>{item.price}</td></tr>) : <Empty colSpan={4} />}</tbody></table></DataTable></div>}
    </>}
    {fillTarget !== null && <div className="modal-backdrop" role="presentation"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="fill-title"><div className="panel-heading"><div><p className="eyebrow">SIMULATED FILL</p><h2 id="fill-title">记录成交 · #{fillTarget}</h2></div><button className="close-button" onClick={() => setFillTarget(null)} aria-label="关闭">×</button></div><form onSubmit={submitFill}><label htmlFor="fill-price">成交价格</label><input id="fill-price" inputMode="decimal" value={fillPrice} onChange={(event) => setFillPrice(event.target.value)} autoFocus required /><div className="modal-actions"><button type="button" className="secondary-button" disabled={busyAction !== ''} onClick={() => setFillTarget(null)}>取消</button><button className="primary-button" disabled={busyAction !== ''} type="submit">{busyAction === `fill-${fillTarget}` ? '处理中…' : '确认成交'}</button></div></form></section></div>}
    <footer><span>ib paper trading</span><span>·</span><span>模拟交易，不连接真实市场</span><span>·</span><span>{lastUpdated ? `最后同步 ${lastUpdated.toLocaleTimeString()}` : '等待同步'}</span></footer>
    </div>
  </main>
}

function PanelTitle({ eyebrow, title }: { eyebrow: string; title: string }) { return <div className="panel-heading"><div><p className="eyebrow">{eyebrow}</p><h2>{title}</h2></div></div> }
function DataTable({ eyebrow, title, action, children }: { eyebrow: string; title: string; action?: ReactNode; children: ReactNode }) { return <section className="data-panel"><div className="panel-heading"><PanelTitle eyebrow={eyebrow} title={title} />{action}</div><div className="table-wrap">{children}</div></section> }
function Empty({ colSpan }: { colSpan: number }) { return <tr><td colSpan={colSpan} className="muted">暂无数据</td></tr> }
function Metric({ label, value, hint, icon, tone }: { label: string; value: string; hint: string; icon: string; tone: string }) { return <div className={`metric-card metric-${tone}`}><div className="metric-top"><span>{label}</span><i>{icon}</i></div><strong>{value}</strong><small>{hint}</small></div> }

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>)
