import { DataTable, Empty } from '../components'
import type { Overview } from '../types'

export default function PositionsPage({ positions, symbols }: { positions: Overview['positions']; symbols: Map<number, string> }) {
  return <div className="single-data-page" id="positions"><DataTable title="持仓" eyebrow="POSITIONS"><table><thead><tr><th>合约</th><th>数量</th><th>平均成本</th></tr></thead><tbody>{positions.length ? positions.map((item) => <tr key={item.conid}><td>{symbols.get(item.conid) ?? `#${item.conid}`}</td><td className={Number(item.position) < 0 ? 'negative' : 'positive'}>{item.position}</td><td>{item.avg_cost ?? '—'}</td></tr>) : <Empty colSpan={3} />}</tbody></table></DataTable></div>
}
