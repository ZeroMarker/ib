import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { DataTable, Empty, Metric, PanelTitle } from '../components'

describe('PanelTitle', () => {
  it('renders the eyebrow and title', () => {
    render(<PanelTitle eyebrow="ORDERS" title="订单" />)
    expect(screen.getByText('ORDERS')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '订单' })).toBeInTheDocument()
  })
})

describe('DataTable', () => {
  it('renders the heading, an optional action and its children', () => {
    const onRefresh = vi.fn()
    render(
      <DataTable
        eyebrow="POSITIONS"
        title="持仓"
        action={<button onClick={onRefresh}>刷新</button>}
      >
        <div>cell</div>
      </DataTable>,
    )
    expect(screen.getByRole('heading', { name: '持仓' })).toBeInTheDocument()
    expect(screen.getByText('cell')).toBeInTheDocument()
    screen.getByRole('button', { name: '刷新' }).click()
    expect(onRefresh).toHaveBeenCalledOnce()
  })
})

describe('Empty', () => {
  it('spans the given column count', () => {
    render(
      <table>
        <tbody>
          <Empty colSpan={4} />
        </tbody>
      </table>,
    )
    const cell = screen.getByText('暂无数据')
    expect(cell.tagName).toBe('TD')
    expect(cell).toHaveAttribute('colspan', '4')
  })
})

describe('Metric', () => {
  it('shows label, value and hint with the requested tone', () => {
    const { container } = render(
      <Metric label="现金余额" value="81,448" hint="USD · 可用资金" icon="◈" tone="lime" />,
    )
    expect(screen.getByText('现金余额')).toBeInTheDocument()
    expect(screen.getByText('81,448')).toBeInTheDocument()
    expect(screen.getByText('USD · 可用资金')).toBeInTheDocument()
    expect(container.querySelector('.metric-card')).toHaveClass('metric-lime')
  })
})
