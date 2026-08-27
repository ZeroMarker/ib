import type { FormEvent } from 'react'
import type { InstallPrompt, View } from '../types'

type LinkSpec = { view: View; href: string; icon: string; label: string }
type NavCountSpec = { view: View; href: string; icon: string; label: string; count?: number }

const sidebarLinks: NavCountSpec[] = [
  { view: 'overview', href: '#overview', icon: '⌂', label: '总览' },
  { view: 'trade', href: '#trade', icon: '⌁', label: '交易终端' },
  { view: 'orders', href: '#orders', icon: '≡', label: '订单管理' },
  { view: 'positions', href: '#positions', icon: '◫', label: '持仓账户' },
  { view: 'fills', href: '#fills', icon: '◌', label: '成交记录' },
]

const mobileLinks: LinkSpec[] = [
  { view: 'overview', href: '#overview', icon: '⌂', label: '总览' },
  { view: 'trade', href: '#trade', icon: '⌁', label: '交易' },
  { view: 'orders', href: '#orders', icon: '≡', label: '订单' },
  { view: 'positions', href: '#positions', icon: '◫', label: '持仓' },
  { view: 'fills', href: '#fills', icon: '◌', label: '成交' },
]

export function Sidebar({ activeView, openOrders, installPrompt, busyAction, onRefresh, onInstall }: { activeView: View; openOrders: number; installPrompt: InstallPrompt | null; busyAction: string; onRefresh: () => void; onInstall: () => void }) {
  return (
    <aside className="app-sidebar">
      <div className="sidebar-brand"><div className="brand-mark small">ib</div><div><strong>ib paper</strong><span>SIMULATION OS</span></div></div>
      <div className="sidebar-section"><p>工作区</p>
        {sidebarLinks.map((item) => (
          <a key={item.view} className={activeView === item.view ? 'sidebar-link active' : 'sidebar-link'} href={item.href}>
            <span>{item.icon}</span>{item.label}{item.view === 'orders' && <b>{openOrders}</b>}
          </a>
        ))}
      </div>
      <div className="sidebar-section"><p>工具</p>
        <button className="sidebar-link sidebar-button" onClick={onRefresh} disabled={busyAction !== ''}><span>↻</span>同步数据</button>
        <button className="sidebar-link sidebar-button" onClick={onInstall} disabled={!installPrompt}><span>⇩</span>安装 PWA</button>
      </div>
      <div className="sidebar-footer"><span className="status-dot" />模拟环境运行中<small>Oracle ledger · session safe · 1-5 切换视图</small></div>
    </aside>
  )
}

export function DashboardHeader({ email, currentView, installPrompt, onInstall, onLogout }: { email: string; currentView: { eyebrow: string; title: string }; installPrompt: InstallPrompt | null; onInstall: () => void; onLogout: () => void }) {
  return (
    <header className="dashboard-header">
      <div className="brand-line"><div className="mobile-brand-mark brand-mark small">ib</div><div><p className="eyebrow">{currentView.eyebrow}</p><h1>{currentView.title}</h1></div></div>
      <div className="header-actions">
        {installPrompt && <button className="install-button" onClick={onInstall}>安装应用</button>}
        <span className="sync-pill"><span className="status-dot" />数据已同步</span>
        <span className="user-pill"><span className="status-dot" />{email}</span>
        <button className="text-button" onClick={onLogout}>退出登录</button>
      </div>
    </header>
  )
}

export function MobileNav({ activeView }: { activeView: View }) {
  return (
    <nav className="mobile-nav" aria-label="移动端工作台导航">
      {mobileLinks.map((item) => (
        <a key={item.view} className={activeView === item.view ? 'active' : ''} href={item.href}><span>{item.icon}</span>{item.label}</a>
      ))}
    </nav>
  )
}

export function VerifyNote({ emailVerified }: { emailVerified: boolean }) {
  return <div className="verify-note">{emailVerified ? '邮箱已验证。' : '邮箱验证将在 Resend 邮件服务接入后开放。'}</div>
}

export function Alerts({ error, notice, busyAction, onRetry }: { error: string; notice: string; busyAction: string; onRetry: () => void }) {
  return (
    <>
      {error && <div className="alert error"><span>{error}</span><button className="alert-action" disabled={busyAction !== ''} onClick={onRetry}>重试</button></div>}
      {notice && <div className="alert success">{notice}</div>}
    </>
  )
}

export function WorkspaceNav({ activeView, openOrders, positionsCount, fillsCount, lastUpdated, busyAction, onRefresh }: { activeView: View; openOrders: number; positionsCount: number; fillsCount: number; lastUpdated: Date | null; busyAction: string; onRefresh: () => void }) {
  const items: Array<{ view: View; href: string; label: string; count?: number }> = [
    { view: 'overview', href: '#overview', label: '总览' },
    { view: 'trade', href: '#trade', label: '交易' },
    { view: 'orders', href: '#orders', label: '订单', count: openOrders },
    { view: 'positions', href: '#positions', label: '持仓', count: positionsCount },
    { view: 'fills', href: '#fills', label: '成交', count: fillsCount },
  ]
  return (
    <nav className="workspace-nav" aria-label="工作台导航">
      {items.map((item) => (
        <a key={item.view} className={activeView === item.view ? 'active' : ''} href={item.href}>{item.label}{item.count !== undefined && <b>{item.count}</b>}</a>
      ))}
      <span className="refresh-meta">{lastUpdated ? `更新于 ${lastUpdated.toLocaleTimeString()}` : '尚未同步'}<button className="text-button" disabled={busyAction !== ''} onClick={onRefresh}>刷新</button></span>
    </nav>
  )
}

export function FillModal({ fillTarget, fillPrice, setFillPrice, busyAction, onClose, onSubmit }: { fillTarget: number | null; fillPrice: string; setFillPrice: (value: string) => void; busyAction: string; onClose: () => void; onSubmit: (event: FormEvent) => void }) {
  if (fillTarget === null) return null
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="modal" role="dialog" aria-modal="true" aria-labelledby="fill-title">
        <div className="panel-heading"><div><p className="eyebrow">SIMULATED FILL</p><h2 id="fill-title">记录成交 · #{fillTarget}</h2></div><button className="close-button" onClick={onClose} aria-label="关闭">×</button></div>
        <form onSubmit={onSubmit}>
          <label htmlFor="fill-price">成交价格</label>
          <input id="fill-price" inputMode="decimal" value={fillPrice} onChange={(event) => setFillPrice(event.target.value)} autoFocus required />
          <div className="modal-actions"><button type="button" className="secondary-button" disabled={busyAction !== ''} onClick={onClose}>取消</button><button className="primary-button" disabled={busyAction !== ''} type="submit">{busyAction === `fill-${fillTarget}` ? '处理中…' : '确认成交'}</button></div>
        </form>
      </section>
    </div>
  )
}

export function DashboardFooter({ lastUpdated }: { lastUpdated: Date | null }) {
  return (
    <footer>
      <span>ib paper trading</span><span>·</span><span>模拟交易，不连接真实市场</span><span>·</span>
      <span>{lastUpdated ? `最后同步 ${lastUpdated.toLocaleTimeString()}` : '等待同步'}</span>
    </footer>
  )
}
