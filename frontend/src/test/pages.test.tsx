import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import FillsPage from '../pages/FillsPage'
import PositionsPage from '../pages/PositionsPage'
import OrdersPage from '../pages/OrdersPage'
import type { Order, Overview } from '../types'

const order = (patch: Partial<Order> = {}): Order => ({
  order_id: 1,
  account_id: 'SIMabc',
  conid: 1,
  side: 'BUY',
  order_type: 'LMT',
  total_quantity: '100',
  filled_quantity: '0',
  status: 'Submitted',
  lmt_price: '185.52',
  aux_price: null,
  ...patch,
})

const symbols = new Map([[1, 'AAPL']])

describe('PositionsPage', () => {
  it('shows the symbol and marks a short position as negative', () => {
    render(
      <PositionsPage
        positions={[
          { conid: 1, position: '100', avg_cost: '185.52' },
          { conid: 2, position: '-50', avg_cost: null },
        ]}
        symbols={new Map([[1, 'AAPL']])}
      />,
    )
    expect(screen.getByText('AAPL')).toBeInTheDocument()
    expect(screen.getByText('100').className).toContain('positive')
    expect(screen.getByText('-50').className).toContain('negative')
    // A null average cost renders a dash rather than "null".
    expect(screen.getByText('—')).toBeInTheDocument()
  })

  it('falls back to the conid when the symbol is unknown', () => {
    render(
      <PositionsPage positions={[{ conid: 99, position: '1', avg_cost: '2' }]} symbols={symbols} />,
    )
    expect(screen.getByText('#99')).toBeInTheDocument()
  })

  it('renders an empty row when there are no positions', () => {
    render(<PositionsPage positions={[]} symbols={symbols} />)
    expect(screen.getByText('暂无数据')).toBeInTheDocument()
  })
})

describe('FillsPage', () => {
  it('lists executions with their order, quantity and price', () => {
    const fills: Overview['fills'] = [
      { exec_id: 'EX1', order_id: 7, quantity: '100', price: '185.52' },
    ]
    render(<FillsPage fills={fills} />)
    const row = screen.getByText('EX1').closest('tr')!
    expect(within(row).getByText('#7')).toBeInTheDocument()
    expect(within(row).getByText('100')).toBeInTheDocument()
    expect(within(row).getByText('185.52')).toBeInTheDocument()
  })

  it('renders an empty row when there are no fills', () => {
    render(<FillsPage fills={[]} />)
    expect(screen.getByText('暂无数据')).toBeInTheDocument()
  })
})

const ordersProps = {
  symbols,
  filteredOrders: [] as Order[],
  pageOrders: [] as Order[],
  orderFilter: 'ALL',
  setOrderFilter: vi.fn(),
  safeOrderPage: 0,
  pageCount: 1,
  setOrderPage: vi.fn(),
  expandedOrder: null as number | null,
  setExpandedOrder: vi.fn(),
  busyAction: '',
  onRefresh: vi.fn(),
  onCancel: vi.fn(),
  onOpenFill: vi.fn(),
}

describe('OrdersPage', () => {
  it('offers cancel and fill only for submittable orders', () => {
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={[order(), order({ order_id: 2, status: 'Filled' })]}
        pageOrders={[order(), order({ order_id: 2, status: 'Filled' })]}
      />,
    )
    expect(screen.getAllByRole('button', { name: '撤单' })).toHaveLength(1)
    expect(screen.getAllByRole('button', { name: '成交' })).toHaveLength(1)
  })

  it('shows a busy label while a cancel is in flight', () => {
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={[order()]}
        pageOrders={[order()]}
        busyAction="cancel-1"
      />,
    )
    expect(screen.getByRole('button', { name: '处理中…' })).toBeDisabled()
  })

  it('asks to expand a collapsed order', async () => {
    const user = userEvent.setup()
    const setExpandedOrder = vi.fn()
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={[order()]}
        pageOrders={[order()]}
        expandedOrder={null}
        setExpandedOrder={setExpandedOrder}
      />,
    )
    await user.click(screen.getByRole('button', { name: '#1' }))
    expect(setExpandedOrder).toHaveBeenCalledWith(1)
  })

  it('asks to collapse an already expanded order', async () => {
    const user = userEvent.setup()
    const setExpandedOrder = vi.fn()
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={[order()]}
        pageOrders={[order()]}
        expandedOrder={1}
        setExpandedOrder={setExpandedOrder}
      />,
    )
    await user.click(screen.getByRole('button', { name: '#1' }))
    expect(setExpandedOrder).toHaveBeenCalledWith(null)
  })

  it('renders the detail row for the expanded order', () => {
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={[order()]}
        pageOrders={[order()]}
        expandedOrder={1}
      />,
    )
    expect(screen.getByText('类型：LMT')).toBeInTheDocument()
    expect(screen.getByText('限价：185.52')).toBeInTheDocument()
    expect(screen.getByText('触发价：—')).toBeInTheDocument()
  })

  it('switches filter and resets to the first page', async () => {
    const user = userEvent.setup()
    const setOrderFilter = vi.fn()
    const setOrderPage = vi.fn()
    render(
      <OrdersPage {...ordersProps} setOrderFilter={setOrderFilter} setOrderPage={setOrderPage} />,
    )
    await user.click(screen.getByRole('button', { name: '已成交' }))
    expect(setOrderFilter).toHaveBeenCalledWith('FILLED')
    expect(setOrderPage).toHaveBeenCalledWith(0)
  })

  it('hides the pager when everything fits on one page', () => {
    render(<OrdersPage {...ordersProps} filteredOrders={[order()]} pageOrders={[order()]} />)
    expect(screen.queryByText(/第 1 \/ 1 页/)).not.toBeInTheDocument()
  })

  it('disables the page buttons at the boundaries', () => {
    const many = Array.from({ length: 20 }, (_, index) => order({ order_id: index + 1 }))
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={many}
        pageOrders={many.slice(0, 8)}
        pageCount={3}
        safeOrderPage={0}
      />,
    )
    expect(screen.getByRole('button', { name: '上一页' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '下一页' })).toBeEnabled()
  })

  it('calls onCancel and onOpenFill with the order', async () => {
    const user = userEvent.setup()
    const onCancel = vi.fn()
    const onOpenFill = vi.fn()
    render(
      <OrdersPage
        {...ordersProps}
        filteredOrders={[order()]}
        pageOrders={[order()]}
        onCancel={onCancel}
        onOpenFill={onOpenFill}
      />,
    )
    await user.click(screen.getByRole('button', { name: '撤单' }))
    await user.click(screen.getByRole('button', { name: '成交' }))
    expect(onCancel).toHaveBeenCalledWith(1)
    expect(onOpenFill).toHaveBeenCalledWith(expect.objectContaining({ order_id: 1 }))
  })
})
