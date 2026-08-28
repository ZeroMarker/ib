import { Metric, PanelTitle } from '../components'
import type { Fill, Overview, View } from '../types'

type InstrumentActivity = Overview['contracts'][number] & { orders: number; position: string }

type Props = {
  overview: Overview
  dataState: 'online' | 'stale'
  openOrders: number
  filledOrders: number
  filledQuantity: number
  cashTotal: number
  positionCost: number
  latestFill?: Fill
  contractActivity: InstrumentActivity[]
  onNavigate: (view: View) => void
}

export default function OverviewPage({ overview, dataState, openOrders, filledOrders, filledQuantity, cashTotal, positionCost, latestFill, contractActivity, onNavigate }: Props) {
  const connectionLabel = dataState === 'online' ? '可用' : '需检查'
  return <>
    <section id="overview" className="account-hero"><div><div className="account-kicker"><span className="live-pulse" />PAPER ACCOUNT <span>·</span> {overview.account.status}</div><h2>准备好测试下一笔交易。</h2><p>这是你的隔离模拟账户，所有订单只会写入模拟账本。</p><div className="account-meta"><span><small>账户编号</small><strong>{overview.account.account_id}</strong></span><span><small>账户类型</small><strong>{overview.account.account_type}</strong></span><span><small>基础币种</small><strong>{overview.account.currency}</strong></span></div></div><div className="account-visual"><div className="ring ring-large"><span>100%</span><small>模拟安全</small></div><span className="visual-caption">REAL MARKET DISCONNECTED</span></div></section>
    <section className="metric-grid"><Metric label="现金余额" value={cashTotal.toLocaleString('en-US', { maximumFractionDigits: 2 })} hint={`${overview.account.currency} · 可用资金`} icon="◈" tone="lime" /><Metric label="持仓成本" value={positionCost.toLocaleString('en-US', { maximumFractionDigits: 2 })} hint={`${overview.positions.length} 个标的`} icon="◒" tone="blue" /><Metric label="开放订单" value={String(openOrders)} hint="等待成交" icon="◎" tone="amber" /><Metric label="累计成交" value={String(filledQuantity)} hint={`${filledOrders} 笔已完成`} icon="✓" tone="violet" /></section>
    <section className="insight-grid"><div className="insight-card insight-primary"><div className="insight-copy"><p className="eyebrow">SESSION SNAPSHOT</p><h2>你的模拟盘正在等待策略。</h2><p>从订单票据开始，验证价格、数量和账务变化。所有操作都可以撤销或重新演练。</p><div className="insight-actions"><a className="primary-link" href="#trade">开始下单 <span aria-hidden="true">→</span></a><a className="quiet-link" href="#orders">查看订单</a></div></div><div className="mini-chart" role="img" aria-label="模拟账户活动趋势"><span /><span /><span /><span /><span /><span /><span /><span /><i /></div></div><div className="status-card"><div className="card-label"><span>系统状态</span><span className={`status-chip status-chip-${dataState}`}>{dataState === 'online' ? 'NORMAL' : 'STALE'}</span></div><div className="status-row"><span>API 服务</span><strong><i className={`status-dot status-dot-${dataState}`} />{connectionLabel}</strong></div><div className="status-row"><span>账本读取</span><strong><i className={`status-dot status-dot-${dataState}`} />{connectionLabel}</strong></div><div className="status-row"><span>撮合模式</span><strong>手动成交</strong></div><div className="status-row"><span>最近成交</span><strong>{latestFill ? `#${latestFill.order_id}` : '暂无'}</strong></div></div></section>
    <section className="watchlist-panel"><div className="panel-heading"><PanelTitle eyebrow="INSTRUMENT MONITOR" title="合约观察" /><div className="panel-heading-note"><span className="demo-badge">SIMULATED</span><span>行情接入待配置</span></div></div><div className="watchlist-grid">{contractActivity.length ? contractActivity.slice(0, 6).map((item, index) => <div className="watch-card" key={item.conid}><div className="watch-card-top"><span className={`watch-icon watch-icon-${index % 4}`} aria-hidden="true">{item.symbol.slice(0, 1)}</span><span className="watch-symbol">{item.symbol}<small>{item.exchange} · {item.currency}</small></span><button type="button" className="watch-more" aria-label={`在交易终端操作 ${item.symbol}`} onClick={() => onNavigate('trade')}>•••</button></div><div className="watch-placeholder"><span>行情等待接入</span><strong>—</strong></div><div className="watch-footer"><span>{item.orders} 笔订单</span><span className={Number(item.position) < 0 ? 'negative' : 'positive'}>{Number(item.position) ? `${item.position} 持仓` : '无持仓'}</span></div></div>) : <div className="empty-state"><span aria-hidden="true">＋</span><strong>添加第一个合约</strong><small>在交易终端中录入合约后，这里会显示监控卡片。</small></div>}</div></section>
  </>
}
