import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useTrading } from '../hooks/useTrading'
import type { Overview } from '../types'

const overview: Overview = {
  account: { account_id: 'SIMabc', account_type: 'MARGIN', currency: 'USD', status: 'ACTIVE' },
  contracts: [
    { conid: 1, symbol: 'AAPL', sec_type: 'STK', exchange: 'SMART', currency: 'USD' },
    { conid: 2, symbol: 'MSFT', sec_type: 'STK', exchange: 'SMART', currency: 'USD' },
  ],
  orders: [
    {
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
    },
    {
      order_id: 2,
      account_id: 'SIMabc',
      conid: 2,
      side: 'SELL',
      order_type: 'MKT',
      total_quantity: '50',
      filled_quantity: '50',
      status: 'Filled',
      lmt_price: null,
      aux_price: null,
    },
  ],
  positions: [{ conid: 1, position: '100', avg_cost: '185.52' }],
  cash: [
    { currency: 'USD', cash: '81448' },
    { currency: 'EUR', cash: '500' },
  ],
  fills: [
    { exec_id: 'EX1', order_id: 2, quantity: '50', price: '400.1' },
    { exec_id: 'EX2', order_id: 1, quantity: '10', price: '185.52' },
  ],
}

/** Respond to every trading call with a canned overview. */
const stubOverview = (payload: Overview = overview) => {
  const mock = vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    json: () => Promise.resolve(payload),
  })
  vi.stubGlobal('fetch', mock)
  return mock
}

/**
 * `onLogout` must be a stable reference: `useTrading` refetches whenever it
 * changes identity, so a fresh `vi.fn()` per render would loop.
 */
const renderTrading = () => {
  const onLogout = vi.fn()
  return { ...renderHook(() => useTrading(onLogout)), onLogout }
}

beforeEach(() => {
  window.location.hash = '#overview'
})

afterEach(() => vi.unstubAllGlobals())

describe('useTrading summary', () => {
  it('counts open and filled orders and sums filled quantity', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.openOrders).toBe(1)
    expect(result.current.filledOrders).toBe(1)
    expect(result.current.filledQuantity).toBe(50)
  })

  it('sums cash in the account base currency only', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    // EUR must not be added to the USD total.
    expect(result.current.cashTotal).toBe(81448)
  })

  it('values positions at absolute quantity times average cost', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.positionCost).toBeCloseTo(18552, 6)
  })

  it('joins per-contract order counts and positions', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    const byConid = new Map(result.current.contractActivity.map((item) => [item.conid, item]))
    expect(byConid.get(1)?.orders).toBe(1)
    expect(byConid.get(1)?.position).toBe('100')
    expect(byConid.get(2)?.orders).toBe(1)
    expect(byConid.get(2)?.position).toBe('0')
  })

  it('takes the most recent fill as the latest', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.latestFill?.exec_id).toBe('EX1')
  })

  it('does not sum positions with a null average cost as zero-cost', async () => {
    stubOverview({
      ...overview,
      positions: [{ conid: 1, position: '10', avg_cost: null }],
    })
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.positionCost).toBe(0)
  })
})

describe('useTrading order filtering and paging', () => {
  it('splits orders by status tab', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.filteredOrders).toHaveLength(2)
    act(() => result.current.setOrderFilter('OPEN'))
    expect(result.current.filteredOrders.map((o) => o.order_id)).toEqual([1])
    act(() => result.current.setOrderFilter('FILLED'))
    expect(result.current.filteredOrders.map((o) => o.order_id)).toEqual([2])
    act(() => result.current.setOrderFilter('CANCELLED'))
    expect(result.current.filteredOrders).toHaveLength(0)
  })

  it('keeps the page index inside range when the filter shrinks', async () => {
    const many: Overview = {
      ...overview,
      orders: Array.from({ length: 20 }, (_, index) => ({
        ...overview.orders[0],
        order_id: index + 1,
      })),
    }
    stubOverview(many)
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.pageCount).toBe(3)
    act(() => result.current.setOrderPage(2))
    expect(result.current.safeOrderPage).toBe(2)
    act(() => result.current.setOrderFilter('FILLED'))
    // 20 open orders narrow to 0 matches; the page must clamp, not go blank.
    expect(result.current.pageCount).toBe(1)
    expect(result.current.safeOrderPage).toBe(0)
  })
})

describe('useTrading contract search', () => {
  it('matches on symbol, conid and exchange, case-insensitively', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    act(() => result.current.setContractQuery('aap'))
    expect(result.current.visibleContracts.map((c) => c.symbol)).toEqual(['AAPL'])
    act(() => result.current.setContractQuery('2'))
    expect(result.current.visibleContracts.map((c) => c.symbol)).toEqual(['MSFT'])
    act(() => result.current.setContractQuery(''))
    expect(result.current.visibleContracts).toHaveLength(2)
  })
})

describe('useTrading order form defaulting', () => {
  it('preselects the first contract once contracts load', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    expect(result.current.orderForm.conid).toBe('1')
  })

  it('does not overwrite a contract the user already chose', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    act(() => result.current.setOrderForm({ ...result.current.orderForm, conid: '2' }))
    expect(result.current.orderForm.conid).toBe('2')
  })
})

describe('useTrading validation', () => {
  it('rejects a non-positive cash amount', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    act(() => result.current.setCashForm({ currency: 'USD', amount: '-5' }))
    act(() => {
      result.current.setCash({ preventDefault: () => {} } as never)
    })
    expect(result.current.error).toContain('正数金额')
  })

  it('rejects a duplicate contract before calling the API', async () => {
    const mock = stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())
    const before = mock.mock.calls.length

    act(() =>
      result.current.setContractForm({
        conid: '1',
        symbol: 'AAPL',
        exchange: 'SMART',
        currency: 'USD',
      }),
    )
    act(() => {
      result.current.addContract({ preventDefault: () => {} } as never)
    })
    expect(result.current.error).toContain('已存在')
    expect(mock.mock.calls.length).toBe(before)
  })

  it('requires a limit price for LMT orders', async () => {
    stubOverview()
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.overview).not.toBeNull())

    act(() =>
      result.current.setOrderForm({ ...result.current.orderForm, order_type: 'LMT', price: '' }),
    )
    act(() => {
      result.current.placeOrder({ preventDefault: () => {} } as never)
    })
    expect(result.current.error).toContain('限价')
  })
})

describe('useTrading error handling', () => {
  it('marks the data stale when the overview request fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        json: () => Promise.resolve({ error: 'database unavailable' }),
      }),
    )
    const { result } = renderTrading()
    await waitFor(() => expect(result.current.loadFailed).toBe(true))

    expect(result.current.error).toBe('database unavailable')
  })

  it('signs the user out on 401 instead of showing an error', async () => {
    const onLogout = vi.fn()
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        json: () => Promise.resolve({ error: 'authentication required' }),
      }),
    )
    const { result } = renderHook(() => useTrading(onLogout))
    await waitFor(() => expect(onLogout).toHaveBeenCalled())

    expect(result.current.error).toBe('')
    expect(result.current.loadFailed).toBe(false)
  })
})
