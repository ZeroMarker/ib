import type { ReactNode } from 'react'

export function PanelTitle({ eyebrow, title }: { eyebrow: string; title: string }) {
  return <div className="panel-heading"><div><p className="eyebrow">{eyebrow}</p><h2>{title}</h2></div></div>
}

export function DataTable({ eyebrow, title, action, children }: { eyebrow: string; title: string; action?: ReactNode; children: ReactNode }) {
  return <section className="data-panel"><div className="panel-heading"><PanelTitle eyebrow={eyebrow} title={title} />{action}</div><div className="table-wrap">{children}</div></section>
}

export function Empty({ colSpan }: { colSpan: number }) {
  return <tr><td colSpan={colSpan} className="muted">暂无数据</td></tr>
}

export function Metric({ label, value, hint, icon, tone }: { label: string; value: string; hint: string; icon: string; tone: string }) {
  return <div className={`metric-card metric-${tone}`}><div className="metric-top"><span>{label}</span><i>{icon}</i></div><strong>{value}</strong><small>{hint}</small></div>
}
