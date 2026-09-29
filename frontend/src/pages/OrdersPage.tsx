import { Fragment } from 'react'
import { DataTable, Empty } from '../components'
import type { Order } from '../types'

type Props = {
  symbols: Map<number, string>
  filteredOrders: Order[]
  pageOrders: Order[]
  orderFilter: string
  setOrderFilter: (value: string) => void
  safeOrderPage: number
  pageCount: number
  setOrderPage: (value: number | ((page: number) => number)) => void
  expandedOrder: number | null
  setExpandedOrder: (value: number | null) => void
  busyAction: string
  onRefresh: () => void
  onCancel: (orderId: number) => void
  onOpenFill: (order: Order) => void
}

const FILTERS: ReadonlyArray<readonly [string, string]> = [
  ['ALL', '全部'],
  ['OPEN', '开放'],
  ['FILLED', '已成交'],
  ['CANCELLED', '已撤销'],
]

/** Statuses that still have remaining quantity and can be acted on. */
const isActionable = (status: string) => ['Submitted', 'PreSubmitted'].includes(status)

function FilterTabs({
  active,
  count,
  onChange,
}: {
  active: string
  count: number
  onChange: (value: string) => void
}) {
  return (
    <div className="table-toolbar">
      <div className="filter-tabs">
        {FILTERS.map(([value, label]) => (
          <button
            key={value}
            className={active === value ? 'filter-tab active' : 'filter-tab'}
            onClick={() => onChange(value)}
          >
            {label}
          </button>
        ))}
      </div>
      <span className="muted">{count} 条</span>
    </div>
  )
}

function OrderActions({
  order,
  busyAction,
  onCancel,
  onOpenFill,
}: {
  order: Order
  busyAction: string
  onCancel: (orderId: number) => void
  onOpenFill: (order: Order) => void
}) {
  if (!isActionable(order.status)) return null
  return (
    <>
      <button
        className="table-action"
        disabled={busyAction !== ''}
        onClick={() => onCancel(order.order_id)}
      >
        {busyAction === `cancel-${order.order_id}` ? '处理中…' : '撤单'}
      </button>
      <button
        className="table-action fill-action"
        disabled={busyAction !== ''}
        onClick={() => onOpenFill(order)}
      >
        成交
      </button>
    </>
  )
}

function OrderDetail({ order }: { order: Order }) {
  return (
    <tr className="detail-row">
      <td colSpan={6}>
        <div className="order-detail">
          <span>类型：{order.order_type}</span>
          <span>限价：{order.lmt_price ?? '—'}</span>
          <span>触发价：{order.aux_price ?? '—'}</span>
          <span>
            成交：{order.filled_quantity} / {order.total_quantity}
          </span>
          <span>合约 ConID：{order.conid}</span>
        </div>
      </td>
    </tr>
  )
}

function Pagination({
  page,
  pageCount,
  onChange,
}: {
  page: number
  pageCount: number
  onChange: (value: number) => void
}) {
  if (pageCount <= 1) return null
  return (
    <div className="pagination">
      <button className="table-action" disabled={page === 0} onClick={() => onChange(page - 1)}>
        上一页
      </button>
      <span>
        第 {page + 1} / {pageCount} 页
      </span>
      <button
        className="table-action"
        disabled={page >= pageCount - 1}
        onClick={() => onChange(page + 1)}
      >
        下一页
      </button>
    </div>
  )
}

export default function OrdersPage({
  symbols,
  filteredOrders,
  pageOrders,
  orderFilter,
  setOrderFilter,
  safeOrderPage,
  pageCount,
  setOrderPage,
  expandedOrder,
  setExpandedOrder,
  busyAction,
  onRefresh,
  onCancel,
  onOpenFill,
}: Props) {
  return (
    <div id="orders" className="single-data-page">
      <DataTable
        title="订单"
        eyebrow="ORDERS"
        action={
          <button className="text-button" disabled={busyAction !== ''} onClick={onRefresh}>
            刷新
          </button>
        }
      >
        <FilterTabs
          active={orderFilter}
          count={filteredOrders.length}
          onChange={(value) => {
            setOrderFilter(value)
            setOrderPage(0)
          }}
        />
        <table>
          <thead>
            <tr>
              <th>订单</th>
              <th>合约</th>
              <th>方向</th>
              <th>数量</th>
              <th>状态</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {pageOrders.length ? (
              pageOrders.map((order) => (
                <Fragment key={`${order.account_id}-${order.order_id}`}>
                  <tr>
                    <td>
                      <button
                        className="row-link"
                        onClick={() =>
                          setExpandedOrder(expandedOrder === order.order_id ? null : order.order_id)
                        }
                      >
                        #{order.order_id}
                      </button>
                    </td>
                    <td>{symbols.get(order.conid) ?? `#${order.conid}`}</td>
                    <td>
                      <span className={order.side === 'BUY' ? 'side buy' : 'side sell'}>
                        {order.side}
                      </span>
                    </td>
                    <td>
                      {order.total_quantity}
                      <small className="table-subvalue"> / {order.filled_quantity} 成交</small>
                    </td>
                    <td>
                      <span className={`status status-${order.status.toLowerCase()}`}>
                        {order.status}
                      </span>
                    </td>
                    <td>
                      <OrderActions
                        order={order}
                        busyAction={busyAction}
                        onCancel={onCancel}
                        onOpenFill={onOpenFill}
                      />
                    </td>
                  </tr>
                  {expandedOrder === order.order_id && <OrderDetail order={order} />}
                </Fragment>
              ))
            ) : (
              <Empty colSpan={6} />
            )}
          </tbody>
        </table>
        <Pagination page={safeOrderPage} pageCount={pageCount} onChange={setOrderPage} />
      </DataTable>
    </div>
  )
}
