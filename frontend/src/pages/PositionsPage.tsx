import { DataTable, Empty } from '../components'
import type { Overview } from '../types'

type Props = {
  positions: Overview['positions']
  symbols: Map<number, string>
}

export default function PositionsPage({ positions, symbols }: Props) {
  return (
    <div className="single-data-page" id="positions">
      <DataTable title="持仓" eyebrow="POSITIONS">
        <table>
          <thead>
            <tr>
              <th>合约</th>
              <th>数量</th>
              <th>平均成本</th>
            </tr>
          </thead>
          <tbody>
            {positions.length ? (
              positions.map((position) => (
                <tr key={position.conid}>
                  <td>{symbols.get(position.conid) ?? `#${position.conid}`}</td>
                  <td className={Number(position.position) < 0 ? 'negative' : 'positive'}>
                    {position.position}
                  </td>
                  <td>{position.avg_cost ?? '—'}</td>
                </tr>
              ))
            ) : (
              <Empty colSpan={3} />
            )}
          </tbody>
        </table>
      </DataTable>
    </div>
  )
}
