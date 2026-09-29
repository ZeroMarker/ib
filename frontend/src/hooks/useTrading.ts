import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { api, json, ApiError } from '../api'
import type { Contract, Order, Overview, Position, View } from '../types'
import { viewFromHash, viewMeta } from '../types'

const LIMIT_TYPES = ['LMT', 'STP_LMT']
const STOP_TYPES = ['STP', 'STP_LMT']
const OPEN_STATUSES = ['Submitted', 'PreSubmitted']
const PAGE_SIZE = 8
const VIEWS: View[] = ['overview', 'trade', 'orders', 'positions', 'fills']
/** Stable empty reference so `symbols` does not change identity per render. */
const EMPTY_SYMBOLS = new Map<number, string>()
const EMPTY_ACTIVITY: InstrumentActivity[] = []

export type ContractForm = { conid: string; symbol: string; exchange: string; currency: string }
export type OrderForm = {
  conid: string
  side: string
  order_type: string
  quantity: string
  price: string
  stopPrice: string
}
export type CashForm = { currency: string; amount: string }
export type InstrumentActivity = Contract & { orders: number; position: string }

const emptyContractForm: ContractForm = {
  conid: '',
  symbol: '',
  exchange: 'SMART',
  currency: 'USD',
}
const defaultOrderForm: OrderForm = {
  conid: '',
  side: 'BUY',
  order_type: 'MKT',
  quantity: '100',
  price: '',
  stopPrice: '',
}
const defaultCashForm: CashForm = { currency: 'USD', amount: '100000' }

/** Reduce the overview payload once per load instead of per render. */
function summarize(overview: Overview) {
  const positionsByConid = new Map(overview.positions.map((item) => [item.conid, item]))
  const orderCountByConid = new Map<number, number>()
  for (const order of overview.orders) {
    orderCountByConid.set(order.conid, (orderCountByConid.get(order.conid) ?? 0) + 1)
  }
  return {
    symbols: new Map(overview.contracts.map((item) => [item.conid, item.symbol])),
    openOrders: overview.orders.filter((order) => OPEN_STATUSES.includes(order.status)).length,
    filledOrders: overview.orders.filter((order) => order.status.toUpperCase() === 'FILLED').length,
    filledQuantity: overview.orders.reduce(
      (total, order) => total + Number(order.filled_quantity),
      0,
    ),
    // Only the account's base currency is meaningful as "available cash";
    // summing across currencies would add unrelated units together.
    cashTotal: Number(
      overview.cash.find((item) => item.currency === overview.account.currency)?.cash ?? 0,
    ),
    positionCost: overview.positions.reduce(
      (total, item) => total + Math.abs(Number(item.position)) * Number(item.avg_cost ?? 0),
      0,
    ),
    latestFill: overview.fills[0],
    contractActivity: overview.contracts.map((contract) => ({
      ...contract,
      orders: orderCountByConid.get(contract.conid) ?? 0,
      position: positionsByConid.get(contract.conid)?.position ?? '0',
    })),
  }
}

function filterOrders(orders: Order[], filter: string) {
  if (filter === 'ALL') return orders
  if (filter === 'OPEN') return orders.filter((order) => OPEN_STATUSES.includes(order.status))
  return orders.filter((order) => order.status.toUpperCase() === filter)
}

export function useTrading(onLogout: () => void) {
  const [activeView, setActiveView] = useState<View>(viewFromHash)
  const [overview, setOverview] = useState<Overview | null>(null)
  const [error, setError] = useState('')
  const [loadFailed, setLoadFailed] = useState(false)
  const [notice, setNotice] = useState('')
  const [contractForm, setContractForm] = useState<ContractForm>(emptyContractForm)
  const [orderForm, setOrderForm] = useState<OrderForm>(defaultOrderForm)
  const [cashForm, setCashForm] = useState<CashForm>(defaultCashForm)
  const [fillTarget, setFillTarget] = useState<number | null>(null)
  const [fillPrice, setFillPrice] = useState('')
  const [contractQuery, setContractQuery] = useState('')
  const [orderFilter, setOrderFilter] = useState('ALL')
  const [orderPage, setOrderPage] = useState(0)
  const [expandedOrder, setExpandedOrder] = useState<number | null>(null)
  const [busyAction, setBusyAction] = useState('')
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null)

  // The keyboard shortcuts need the current `activeView` without
  // re-subscribing on every render. Mirroring into a ref inside an effect
  // (rather than during render) keeps the listener stable while always
  // reading fresh values.
  const activeViewRef = useRef(activeView)
  const busyActionRef = useRef(busyAction)
  useEffect(() => {
    activeViewRef.current = activeView
  }, [activeView])
  useEffect(() => {
    busyActionRef.current = busyAction
  }, [busyAction])

  useEffect(() => {
    const syncView = () => setActiveView(viewFromHash())
    window.addEventListener('hashchange', syncView)
    if (!window.location.hash) window.history.replaceState(null, '', '#overview')
    return () => window.removeEventListener('hashchange', syncView)
  }, [])

  useEffect(() => {
    requestAnimationFrame(() =>
      document.getElementById(activeView)?.scrollIntoView({ behavior: 'smooth', block: 'start' }),
    )
  }, [activeView])

  // Keep the browser tab title in sync with the active workspace view.
  useEffect(() => {
    document.title = `${viewMeta[activeView].title} · ib paper`
  }, [activeView])

  // Responses from superseded loads must not overwrite a newer overview, so
  // each load takes a sequence number and only the latest may commit.
  const loadSeq = useRef(0)
  const load = useCallback(async () => {
    const sequence = ++loadSeq.current
    try {
      const data = await api<Overview>('trading/overview')
      if (sequence !== loadSeq.current) return true
      setOverview(data)
      setError('')
      setLoadFailed(false)
      setLastUpdated(new Date())
      return true
    } catch (reason) {
      if (sequence !== loadSeq.current) return false
      if (reason instanceof ApiError && reason.status === 401) {
        onLogout()
        return false
      }
      setLoadFailed(true)
      setError(reason instanceof Error ? reason.message : '无法读取交易数据')
      return false
    }
  }, [onLogout])

  // Initial load on mount. The ledger is external state, not something
  // derivable during render, so fetching it requires an effect. `load` awaits
  // the network before touching any state, so there is no synchronous
  // setState and no cascading render.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void load()
  }, [load])

  // The order ticket needs a default contract once contracts are known, but
  // deriving it during render would fight the user after every keystroke.
  // Tracking "was a default applied yet" in state lets a single conditional
  // update happen where the data changes, not in an effect.
  const [defaultedConid, setDefaultedConid] = useState<string | null>(null)
  const contractsReady = Boolean(overview?.contracts.length)
  if (contractsReady && defaultedConid === null) {
    setDefaultedConid(String(overview!.contracts[0].conid))
    setOrderForm((form) => ({ ...form, conid: form.conid || String(overview!.contracts[0].conid) }))
  }

  const refresh = useCallback(async () => {
    if (busyActionRef.current) return
    setBusyAction('refresh')
    setNotice('')
    try {
      if (await load()) setNotice('数据已刷新。')
    } finally {
      setBusyAction('')
    }
  }, [load])

  // Terminal shortcuts: 1-5 switch views, R refreshes, Esc closes the fill
  // modal. Subscribed once; the refs above supply the current state.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return
      const target = event.target as HTMLElement | null
      if (
        target &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'SELECT' ||
          target.tagName === 'TEXTAREA' ||
          target.isContentEditable)
      ) {
        return
      }
      const index = ['1', '2', '3', '4', '5'].indexOf(event.key)
      if (index !== -1) {
        const view = VIEWS[index]
        if (view !== activeViewRef.current) {
          event.preventDefault()
          window.location.hash = `#${view}`
        }
        return
      }
      if (event.key === 'r' || event.key === 'R') {
        event.preventDefault()
        refresh()
        return
      }
      if (event.key === 'Escape') setFillTarget(null)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [refresh])

  const summary = useMemo(() => (overview ? summarize(overview) : null), [overview])

  const visibleContracts = useMemo(() => {
    const contracts = overview?.contracts ?? []
    const query = contractQuery.trim().toLowerCase()
    if (!query) return contracts
    return contracts.filter((item) =>
      `${item.symbol} ${item.conid} ${item.exchange}`.toLowerCase().includes(query),
    )
  }, [overview, contractQuery])

  const filteredOrders = useMemo(
    () => (overview ? filterOrders(overview.orders, orderFilter) : []),
    [overview, orderFilter],
  )
  const pageCount = Math.max(1, Math.ceil(filteredOrders.length / PAGE_SIZE))
  const safeOrderPage = Math.min(orderPage, pageCount - 1)
  const pageOrders = useMemo(
    () => filteredOrders.slice(safeOrderPage * PAGE_SIZE, (safeOrderPage + 1) * PAGE_SIZE),
    [filteredOrders, safeOrderPage],
  )

  const fail = (message: string) => {
    setError(message)
    setNotice('')
  }

  const action = async (work: () => Promise<void>, success: string, actionName = 'action') => {
    if (busyActionRef.current) return
    setBusyAction(actionName)
    try {
      await work()
      setNotice(success)
      setError('')
      await load()
    } catch (reason) {
      if (reason instanceof ApiError && reason.status === 401) {
        onLogout()
        return
      }
      setError(reason instanceof Error ? reason.message : '操作失败')
    } finally {
      setBusyAction('')
    }
  }

  const addContract = (event: FormEvent) => {
    event.preventDefault()
    const conid = Number(contractForm.conid)
    const symbol = contractForm.symbol.trim().toUpperCase()
    if (!Number.isInteger(conid) || conid <= 0 || !symbol) {
      fail('请填写有效的合约 ConID 和代码。')
      return
    }
    if (
      overview?.contracts.some(
        (item) => item.conid === conid || item.symbol.toUpperCase() === symbol,
      )
    ) {
      fail('该合约已存在，请直接从下方选择。')
      return
    }
    action(
      async () => {
        await api('trading/contracts', json({ ...contractForm, conid, symbol, sec_type: 'STK' }))
        setContractForm(emptyContractForm)
      },
      '合约已添加。',
      'contract',
    )
  }

  const placeOrder = (event: FormEvent) => {
    event.preventDefault()
    const quantity = Number(orderForm.quantity)
    const needsLimit = LIMIT_TYPES.includes(orderForm.order_type)
    const needsStop = STOP_TYPES.includes(orderForm.order_type)
    const price = Number(orderForm.price)
    const stopPrice = Number(orderForm.stopPrice)
    if (!orderForm.conid || !Number.isFinite(quantity) || quantity <= 0) {
      fail('请选择合约并填写有效数量。')
      return
    }
    if (needsLimit && (!Number.isFinite(price) || price <= 0)) {
      fail('该订单必须填写有效限价。')
      return
    }
    if (needsStop && (!Number.isFinite(stopPrice) || stopPrice <= 0)) {
      fail('该订单必须填写有效触发价。')
      return
    }
    action(
      async () => {
        await api(
          'trading/orders',
          json({
            conid: Number(orderForm.conid),
            side: orderForm.side,
            order_type: orderForm.order_type,
            quantity: orderForm.quantity,
            lmt_price: needsLimit ? orderForm.price : null,
            aux_price: needsStop ? orderForm.stopPrice : null,
          }),
        )
      },
      '订单已提交。',
      'order',
    )
  }

  const cancel = (orderId: number) =>
    action(
      async () => {
        await api(`trading/orders/${orderId}/cancel`, { method: 'POST' })
      },
      '订单已撤销。',
      `cancel-${orderId}`,
    )

  const openFill = (order: Order) => {
    setFillTarget(order.order_id)
    setFillPrice(order.lmt_price ?? '')
  }

  const submitFill = (event: FormEvent) => {
    event.preventDefault()
    const price = Number(fillPrice)
    if (fillTarget === null || !Number.isFinite(price) || price <= 0) {
      fail('请输入有效的成交价格。')
      return
    }
    action(
      async () => {
        await api(`trading/orders/${fillTarget}/fill`, json({ price: fillPrice }))
        setFillTarget(null)
        setFillPrice('')
      },
      '模拟成交已记账。',
      `fill-${fillTarget}`,
    )
  }

  const setCash = (event: FormEvent) => {
    event.preventDefault()
    const amount = Number(cashForm.amount)
    if (!/^[a-zA-Z]{3}$/.test(cashForm.currency) || !Number.isFinite(amount) || amount <= 0) {
      fail('请填写有效的币种和正数金额。')
      return
    }
    action(
      async () => {
        await api('trading/cash', json({ ...cashForm, currency: cashForm.currency.toUpperCase() }))
      },
      '现金余额已更新。',
      'cash',
    )
  }

  const logout = async () => {
    await api('auth/logout', { method: 'POST' }).catch(() => {})
    onLogout()
  }

  return {
    activeView,
    overview,
    error,
    loadFailed,
    notice,
    contractForm,
    setContractForm,
    orderForm,
    setOrderForm,
    cashForm,
    setCashForm,
    fillTarget,
    setFillTarget,
    fillPrice,
    setFillPrice,
    contractQuery,
    setContractQuery,
    orderFilter,
    setOrderFilter,
    orderPage,
    setOrderPage,
    expandedOrder,
    setExpandedOrder,
    busyAction,
    lastUpdated,
    pageCount,
    safeOrderPage,
    pageOrders,
    symbols: summary?.symbols ?? EMPTY_SYMBOLS,
    visibleContracts,
    filteredOrders,
    openOrders: summary?.openOrders ?? 0,
    filledOrders: summary?.filledOrders ?? 0,
    filledQuantity: summary?.filledQuantity ?? 0,
    cashTotal: summary?.cashTotal ?? 0,
    positionCost: summary?.positionCost ?? 0,
    latestFill: summary?.latestFill,
    contractActivity: summary?.contractActivity ?? EMPTY_ACTIVITY,
    load,
    action,
    refresh,
    addContract,
    placeOrder,
    cancel,
    openFill,
    submitFill,
    setCash,
    logout,
  }
}

export type { Position }
