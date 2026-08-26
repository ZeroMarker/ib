import { StrictMode, type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'
import FillsPage from './pages/FillsPage'
import OrdersPage from './pages/OrdersPage'
import OverviewPage from './pages/OverviewPage'
import PositionsPage from './pages/PositionsPage'
import TradePage, { type CashForm, type ContractForm, type OrderForm } from './pages/TradePage'
import type { AuthUser, InstallPrompt, Order, Overview, View } from './types'
import { viewFromHash, viewMeta } from './types'

class ApiError extends Error {
  status: number
  constructor(status: number, message: string) { super(message); this.status = status }
}

const api = async <T,>(path: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(`api/${path}`, { credentials: 'same-origin', ...init })
  const data = await response.json().catch(() => undefined)
  if (!response.ok) throw new ApiError(response.status, data?.error ?? '请求失败，请稍后重试。')
  return data as T
}

const json = (body: unknown): RequestInit => ({ method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })

function App() {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [booting, setBooting] = useState(true)
  const [installPrompt, setInstallPrompt] = useState<InstallPrompt | null>(null)

  useEffect(() => {
    if ('serviceWorker' in navigator) navigator.serviceWorker.register('./sw.js?v=20260825-ux5').catch(() => {})
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
  // Ignore responses from superseded loads: concurrent refreshes must not let
  // a stale overview overwrite a newer one.
  const loadSeq = useRef(0)
  const load = async () => {
    const sequence = ++loadSeq.current
    try {
      const data = await api<Overview>('trading/overview')
      if (sequence !== loadSeq.current) return
      setOverview(data); setError(''); setLastUpdated(new Date())
    } catch (reason) {
      if (sequence !== loadSeq.current) return
      if (reason instanceof ApiError && reason.status === 401) return onLogout()
      setError(reason instanceof Error ? reason.message : '无法读取交易数据')
    }
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
  const cashTotal = overview ? Number(overview.cash.find((item) => item.currency === overview.account.currency)?.cash ?? 0) : 0
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
    catch (reason) {
      if (reason instanceof ApiError && reason.status === 401) return onLogout()
      setError(reason instanceof Error ? reason.message : '操作失败')
    }
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
  const install = async () => { if (!installPrompt) return; try { await installPrompt.prompt(); await installPrompt.userChoice } catch { /* 用户关闭或浏览器拒绝安装提示 */ } finally { onInstalled() } }
  const cancel = (orderId: number) => action(async () => { await api(`trading/orders/${orderId}/cancel`, { method: 'POST' }) }, '订单已撤销。', `cancel-${orderId}`)
  const submitFill = (event: FormEvent) => { event.preventDefault(); const price = Number(fillPrice); if (fillTarget === null || !Number.isFinite(price) || price <= 0) return setError('请输入有效的成交价格。'); return action(async () => { await api(`trading/orders/${fillTarget}/fill`, json({ price: fillPrice })); setFillTarget(null); setFillPrice('') }, '模拟成交已记账。', `fill-${fillTarget}`) }
  const setCash = (event: FormEvent) => { event.preventDefault(); const amount = Number(cashForm.amount); if (!/^[a-zA-Z]{3}$/.test(cashForm.currency) || !Number.isFinite(amount) || amount <= 0) return setError('请填写有效的币种和正数金额。'); return action(async () => { await api('trading/cash', json({ ...cashForm, currency: cashForm.currency.toUpperCase() })) }, '现金余额已更新。', 'cash') }
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
        <nav className="workspace-nav" aria-label="工作台导航"><a className={activeView === 'overview' ? 'active' : ''} href="#overview">总览</a><a className={activeView === 'trade' ? 'active' : ''} href="#trade">交易</a><a className={activeView === 'orders' ? 'active' : ''} href="#orders">订单 <b>{openOrders}</b></a><a className={activeView === 'positions' ? 'active' : ''} href="#positions">持仓 <b>{overview.positions.length}</b></a><a className={activeView === 'fills' ? 'active' : ''} href="#fills">成交 <b>{overview.fills.length}</b></a><span className="refresh-meta">{lastUpdated ? `更新于 ${lastUpdated.toLocaleTimeString()}` : '尚未同步'}<button className="text-button" disabled={busyAction !== ''} onClick={() => action(load, '数据已刷新。', 'refresh')}>刷新</button></span></nav>
        {activeView === 'overview' && <OverviewPage overview={overview} openOrders={openOrders} filledOrders={filledOrders} filledQuantity={filledQuantity} cashTotal={cashTotal} positionCost={positionCost} latestFill={latestFill} contractActivity={contractActivity} onNavigate={(view) => { window.location.hash = `#${view}` }} />}
        {activeView === 'trade' && <TradePage overview={overview} contractForm={contractForm} setContractForm={setContractForm} orderForm={orderForm} setOrderForm={setOrderForm} cashForm={cashForm} setCashForm={setCashForm} contractQuery={contractQuery} setContractQuery={setContractQuery} visibleContracts={visibleContracts} busyAction={busyAction} onAddContract={addContract} onPlaceOrder={placeOrder} onSetCash={setCash} />}
        {activeView === 'orders' && <OrdersPage symbols={symbols} filteredOrders={filteredOrders} pageOrders={pageOrders} orderFilter={orderFilter} setOrderFilter={setOrderFilter} safeOrderPage={safeOrderPage} pageCount={pageCount} setOrderPage={setOrderPage} expandedOrder={expandedOrder} setExpandedOrder={setExpandedOrder} busyAction={busyAction} onRefresh={() => action(load, '订单已刷新。', 'refresh')} onCancel={cancel} onOpenFill={(order: Order) => { setFillTarget(order.order_id); setFillPrice(order.lmt_price ?? '') }} />}
        {activeView === 'positions' && <PositionsPage positions={overview.positions} symbols={symbols} />}
        {activeView === 'fills' && <FillsPage fills={overview.fills} />}
      </>}
    {fillTarget !== null && <div className="modal-backdrop" role="presentation"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="fill-title"><div className="panel-heading"><div><p className="eyebrow">SIMULATED FILL</p><h2 id="fill-title">记录成交 · #{fillTarget}</h2></div><button className="close-button" onClick={() => setFillTarget(null)} aria-label="关闭">×</button></div><form onSubmit={submitFill}><label htmlFor="fill-price">成交价格</label><input id="fill-price" inputMode="decimal" value={fillPrice} onChange={(event) => setFillPrice(event.target.value)} autoFocus required /><div className="modal-actions"><button type="button" className="secondary-button" disabled={busyAction !== ''} onClick={() => setFillTarget(null)}>取消</button><button className="primary-button" disabled={busyAction !== ''} type="submit">{busyAction === `fill-${fillTarget}` ? '处理中…' : '确认成交'}</button></div></form></section></div>}
    <footer><span>ib paper trading</span><span>·</span><span>模拟交易，不连接真实市场</span><span>·</span><span>{lastUpdated ? `最后同步 ${lastUpdated.toLocaleTimeString()}` : '等待同步'}</span></footer>
    </div>
  </main>
}

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>)
