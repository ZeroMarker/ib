import { DataTable, Empty } from '../components'
import type { Overview } from '../types'

export default function FillsPage({ fills }: { fills: Overview['fills'] }) {
  return <div className="single-data-page" id="fills"><DataTable title="成交" eyebrow="FILLS"><table><thead><tr><th>执行 ID</th><th>订单</th><th>数量</th><th>价格</th></tr></thead><tbody>{fills.length ? fills.map((item) => <tr key={item.exec_id}><td>{item.exec_id}</td><td>#{item.order_id}</td><td>{item.quantity}</td><td>{item.price}</td></tr>) : <Empty colSpan={4} />}</tbody></table></DataTable></div>
}
