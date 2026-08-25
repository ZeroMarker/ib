import { StrictMode, type FormEvent, type ReactNode, useEffect, useMemo, useState } from 'react'
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
    if ('serviceWorker' in navigator) navigator.serviceWorker.register('./sw.js').catch(() => {})
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
  const [overview, setOverview] = useState<Overview | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [contractForm, setContractForm] = useState({ conid: '', symbol: '', exchange: 'SMART', currency: 'USD' })
  const [orderForm, setOrderForm] = useState({ conid: '', side: 'BUY', order_type: 'MKT', quantity: '100', price: '' })
  const [cashForm, setCashForm] = useState({ currency: 'USD', amount: '100000' })
  const [fillTarget, setFillTarget] = useState<number | null>(null)
  const [fillPrice, setFillPrice] = useState('')

  const load = async () => {
    try { setOverview(await api<Overview>('trading/overview')); setError('') }
    catch (reason) { setError(reason instanceof Error ? reason.message : '无法读取交易数据') }
  }
  useEffect(() => { load() }, [])
  useEffect(() => {
    if (overview?.contracts.length && !overview.contracts.some((item) => String(item.conid) === orderForm.conid)) {
      setOrderForm((form) => ({ ...form, conid: String(overview.contracts[0].conid) }))
    }
  }, [overview])

  const symbols = useMemo(() => new Map((overview?.contracts ?? []).map((item) => [item.conid, item.symbol])), [overview])
  const openOrders = overview?.orders.filter((order) => ['Submitted', 'PreSubmitted'].includes(order.status)).length ?? 0
  const cashTotal = overview?.cash.reduce((total, item) => total + Number(item.cash), 0) ?? 0
  const action = async (work: () => Promise<void>, success: string) => {
    try { await work(); setNotice(success); setError(''); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : '操作失败') }
  }
  const addContract = (event: FormEvent) => { event.preventDefault(); return action(async () => { await api('trading/contracts', json({ ...contractForm, conid: Number(contractForm.conid), sec_type: 'STK' })); setContractForm({ conid: '', symbol: '', exchange: 'SMART', currency: 'USD' }) }, '合约已添加。') }
  const placeOrder = (event: FormEvent) => { event.preventDefault(); return action(async () => { await api('trading/orders', json({ conid: Number(orderForm.conid), side: orderForm.side, order_type: orderForm.order_type, quantity: orderForm.quantity, lmt_price: orderForm.order_type === 'LMT' ? orderForm.price || null : null, aux_price: null })) }, '订单已提交。') }
  const setCash = (event: FormEvent) => { event.preventDefault(); return action(async () => { await api('trading/cash', json(cashForm)) }, '现金余额已更新。') }
  const cancel = (orderId: number) => action(async () => { await api(`trading/orders/${orderId}/cancel`, { method: 'POST' }) }, '订单已撤销。')
  const submitFill = (event: FormEvent) => { event.preventDefault(); if (fillTarget === null || !fillPrice) return; return action(async () => { await api(`trading/orders/${fillTarget}/fill`, json({ price: fillPrice })); setFillTarget(null); setFillPrice('') }, '模拟成交已记账。') }
  const install = async () => { if (!installPrompt) return; await installPrompt.prompt(); await installPrompt.userChoice; onInstalled() }
  const logout = async () => { await api('auth/logout', { method: 'POST' }).catch(() => {}); onLogout() }

  return <main className="dashboard-page">
    <header className="dashboard-header"><div className="brand-line"><div className="brand-mark small">ib</div><div><p className="eyebrow">YOUR SIMULATION SPACE</p><h1>交易工作台</h1></div></div><div className="header-actions">{installPrompt && <button className="install-button" onClick={install}>安装应用</button>}<span className="user-pill"><span className="status-dot" />{user.email}</span><button className="text-button" onClick={logout}>退出登录</button></div></header>
    <div className="verify-note">{user.email_verified ? '邮箱已验证。' : '邮箱验证将在 Resend 邮件服务接入后开放。'}</div>
    {error && <div className="alert error">{error}</div>}{notice && <div className="alert success">{notice}</div>}
    {!overview ? <div className="loading-card">正在读取账户数据…</div> : <>
      <section className="account-banner"><span>模拟账户</span><strong>{overview.account.account_id}</strong><small>{overview.account.account_type} · {overview.account.currency} · {overview.account.status}</small></section>
      <nav className="workspace-nav" aria-label="工作台导航"><a href="#trade">交易</a><a href="#orders">订单 <b>{openOrders}</b></a><a href="#positions">持仓 <b>{overview.positions.length}</b></a><a href="#fills">成交 <b>{overview.fills.length}</b></a></nav>
      <section className="metric-grid"><Metric label="现金余额" value={cashTotal ? cashTotal.toLocaleString('en-US', { maximumFractionDigits: 2 }) : '—'} hint={overview.account.currency} /><Metric label="开放订单" value={String(openOrders)} hint="等待成交" /><Metric label="持仓标的" value={String(overview.positions.length)} hint="当前账户" /><Metric label="成交次数" value={String(overview.fills.length)} hint="已记账" /></section>
      <div id="trade" className="trade-layout">
        <section className="trade-panel"><PanelTitle eyebrow="PAPER ORDER" title="提交模拟订单" />
          <form className="compact-form" onSubmit={addContract}><label>新增合约</label><div className="field-row"><input inputMode="numeric" placeholder="ConID" value={contractForm.conid} onChange={(event) => setContractForm({ ...contractForm, conid: event.target.value })} required /><input placeholder="AAPL" value={contractForm.symbol} onChange={(event) => setContractForm({ ...contractForm, symbol: event.target.value })} required /></div><div className="field-row"><input value={contractForm.exchange} onChange={(event) => setContractForm({ ...contractForm, exchange: event.target.value })} placeholder="交易所" /><input value={contractForm.currency} maxLength={3} onChange={(event) => setContractForm({ ...contractForm, currency: event.target.value })} placeholder="币种" /></div><button className="secondary-button">添加合约</button></form>
          <form className="compact-form" onSubmit={placeOrder}><label>合约</label><select value={orderForm.conid} onChange={(event) => setOrderForm({ ...orderForm, conid: event.target.value })} required><option value="">请选择合约</option>{overview.contracts.map((item) => <option key={item.conid} value={item.conid}>{item.symbol} · {item.sec_type} · {item.exchange}</option>)}</select><div className="field-row"><div><label>方向</label><select value={orderForm.side} onChange={(event) => setOrderForm({ ...orderForm, side: event.target.value })}><option>BUY</option><option>SELL</option></select></div><div><label>类型</label><select value={orderForm.order_type} onChange={(event) => setOrderForm({ ...orderForm, order_type: event.target.value })}><option>MKT</option><option>LMT</option></select></div></div><div className="field-row"><div><label>数量</label><input inputMode="decimal" value={orderForm.quantity} onChange={(event) => setOrderForm({ ...orderForm, quantity: event.target.value })} required /></div><div><label>价格</label><input inputMode="decimal" placeholder="市价可留空" value={orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, price: event.target.value })} /></div></div><button className="primary-button">提交订单</button></form>
        </section>
        <section className="trade-panel"><PanelTitle eyebrow="CASH LEDGER" title="现金账本" /><form className="compact-form" onSubmit={setCash}><div className="field-row"><div><label>币种</label><input maxLength={3} value={cashForm.currency} onChange={(event) => setCashForm({ ...cashForm, currency: event.target.value })} required /></div><div><label>余额</label><input inputMode="decimal" value={cashForm.amount} onChange={(event) => setCashForm({ ...cashForm, amount: event.target.value })} required /></div></div><button className="secondary-button">设置模拟余额</button></form><div className="mini-list">{overview.cash.length ? overview.cash.map((item) => <div key={item.currency}><span>{item.currency}</span><strong>{item.cash}</strong></div>) : <span className="muted">暂无余额，可在上方设置模拟余额。</span>}</div></section>
      </div>
      <div id="orders"><DataTable title="订单" eyebrow="ORDERS" action={<button className="text-button" onClick={load}>刷新</button>}><table><thead><tr><th>订单</th><th>合约</th><th>方向</th><th>数量</th><th>状态</th><th /></tr></thead><tbody>{overview.orders.length ? overview.orders.map((order) => <tr key={`${order.account_id}-${order.order_id}`}><td>#{order.order_id}</td><td>{symbols.get(order.conid) ?? `#${order.conid}`}</td><td><span className={order.side === 'BUY' ? 'side buy' : 'side sell'}>{order.side}</span></td><td>{order.total_quantity}</td><td><span className={`status status-${order.status.toLowerCase()}`}>{order.status}</span></td><td>{['Submitted', 'PreSubmitted'].includes(order.status) && <><button className="table-action" onClick={() => cancel(order.order_id)}>撤单</button><button className="table-action fill-action" onClick={() => { setFillTarget(order.order_id); setFillPrice(order.lmt_price ?? '') }}>成交</button></>}</td></tr>) : <Empty colSpan={6} />}</tbody></table></DataTable></div>
      <div className="data-columns"><div id="positions"><DataTable title="持仓" eyebrow="POSITIONS"><table><thead><tr><th>合约</th><th>数量</th><th>平均成本</th></tr></thead><tbody>{overview.positions.length ? overview.positions.map((item) => <tr key={item.conid}><td>{symbols.get(item.conid) ?? `#${item.conid}`}</td><td className={Number(item.position) < 0 ? 'negative' : 'positive'}>{item.position}</td><td>{item.avg_cost ?? '—'}</td></tr>) : <Empty colSpan={3} />}</tbody></table></DataTable></div><div id="fills"><DataTable title="成交" eyebrow="FILLS"><table><thead><tr><th>执行 ID</th><th>订单</th><th>数量</th><th>价格</th></tr></thead><tbody>{overview.fills.length ? overview.fills.map((item) => <tr key={item.exec_id}><td>{item.exec_id}</td><td>#{item.order_id}</td><td>{item.quantity}</td><td>{item.price}</td></tr>) : <Empty colSpan={4} />}</tbody></table></DataTable></div></div>
    </>}
    {fillTarget !== null && <div className="modal-backdrop" role="presentation"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="fill-title"><div className="panel-heading"><div><p className="eyebrow">SIMULATED FILL</p><h2 id="fill-title">记录成交 · #{fillTarget}</h2></div><button className="close-button" onClick={() => setFillTarget(null)} aria-label="关闭">×</button></div><form onSubmit={submitFill}><label htmlFor="fill-price">成交价格</label><input id="fill-price" inputMode="decimal" value={fillPrice} onChange={(event) => setFillPrice(event.target.value)} autoFocus required /><div className="modal-actions"><button type="button" className="secondary-button" onClick={() => setFillTarget(null)}>取消</button><button className="primary-button" type="submit">确认成交</button></div></form></section></div>}
    <footer>ib · simulation trading platform</footer>
  </main>
}

function PanelTitle({ eyebrow, title }: { eyebrow: string; title: string }) { return <div className="panel-heading"><div><p className="eyebrow">{eyebrow}</p><h2>{title}</h2></div></div> }
function DataTable({ eyebrow, title, action, children }: { eyebrow: string; title: string; action?: ReactNode; children: ReactNode }) { return <section className="data-panel"><div className="panel-heading"><PanelTitle eyebrow={eyebrow} title={title} />{action}</div><div className="table-wrap">{children}</div></section> }
function Empty({ colSpan }: { colSpan: number }) { return <tr><td colSpan={colSpan} className="muted">暂无数据</td></tr> }
function Metric({ label, value, hint }: { label: string; value: string; hint: string }) { return <div className="metric-card"><span>{label}</span><strong>{value}</strong><small>{hint}</small></div> }

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>)
