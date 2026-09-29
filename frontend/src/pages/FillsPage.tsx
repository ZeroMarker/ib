import { DataTable, Empty } from '../components'
import type { Overview } from '../types'

type Props = {
  fills: Overview['fills']
}

export default function FillsPage({ fills }: Props) {
  return (
    <div className="single-data-page" id="fills">
      <DataTable title="成交" eyebrow="FILLS">
        <table>
          <thead>
            <tr>
              <th>执行 ID</th>
              <th>订单</th>
              <th>数量</th>
              <th>价格</th>
            </tr>
          </thead>
          <tbody>
            {fills.length ? (
              fills.map((fill) => (
                <tr key={fill.exec_id}>
                  <td>{fill.exec_id}</td>
                  <td>#{fill.order_id}</td>
                  <td>{fill.quantity}</td>
                  <td>{fill.price}</td>
                </tr>
              ))
            ) : (
              <Empty colSpan={4} />
            )}
          </tbody>
        </table>
      </DataTable>
    </div>
  )
}
