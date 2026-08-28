import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { api, json, ApiError } from '../api'
import type { Overview, View } from '../types'
import { viewFromHash, viewMeta } from '../types'

export function useTrading(onLogout: () => void) {
  const [activeView, setActiveView] = useState<View>(viewFromHash)
  const [overview, setOverview] = useState<Overview | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [contractForm, setContractForm] = useState({ conid: '', symbol: '', exchange: 'SMART', currency: 'USD' })
  const [orderForm, setOrderForm] = useState({ conid: '', side: 'BUY', order_type: 'MKT', quantity: '100', price: '', stopPrice: '' })
  const [cashForm, setCashForm] = useState({ currency: 'USD', amount: '100000' })
  const [fillTarget, setFillTarget] = useState<number | null>(null)
  const [fillPrice, setFillPrice] = useState('')
  const [contractQuery, setContractQuery] = useState('')
  const [orderFilter, setOrderFilter] = useState('ALL')
  const [orderPage, setOrderPage] = useState(0)
  const [expandedOrder, setExpandedOrder] = useState<number | null>(null)
  const [busyAction, setBusyAction] = useState('')
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null)

  useEffect(() => {
    const syncView = () => setActiveView(viewFromHash())
    window.addEventListener('hashchange', syncView)
    if (!window.location.hash) window.history.replaceState(null, '', '#overview')
    return () => window.removeEventListener('hashchange', syncView)
  }, [])
  useEffect(() => {
    requestAnimationFrame(() => document.getElementById(activeView)?.scrollIntoView({ behavior: 'smooth', block: 'start' }))
  }, [activeView])

  // Ignore responses from superseded loads: concurrent refreshes must not let
  // a stale overview overwrite a newer one.
  const loadSeq = useRef(0)
  const load = async () => {
    const sequence = ++loadSeq.current
    try {
      const data = await api<Overview>('trading/overview')
      if (sequence !== loadSeq.current) return
      setOverview(data); setError(''); setLastUpdated(new Date())
    } catch (reason) {
      if (sequence !== loadSeq.current) return
      if (reason instanceof ApiError && reason.status === 401) return onLogout()
      setError(reason instanceof Error ? reason.message : '无法读取交易数据')
    }
  }
  useEffect(() => { load() }, [])
  useEffect(() => {
    if (overview?.contracts.length && !overview.contracts.some((item) => String(item.conid) === orderForm.conid)) {
      setOrderForm((form) => ({ ...form, conid: String(overview.contracts[0].conid) }))
    }
  }, [overview])
  // Keep the browser tab title in sync with the active workspace view.
  useEffect(() => {
    document.title = `${viewMeta[activeView].title} · ib paper`
  }, [activeView])
  // Terminal shortcuts: 1-5 switch views, R refresh, Esc dismiss the fill modal.
  // Re-subscribes each render so the handler never closes over stale state.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      if (event.metaKey || event.ctrlKey || event.altKey) return
      if (target && (target.tagName === 'INPUT' || target.tagName === 'SELECT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
      const views: View[] = ['overview', 'trade', 'orders', 'positions', 'fills']
      const index = ['1', '2', '3', '4', '5'].indexOf(event.key)
      if (index !== -1) {
        if (views[index] !== activeView) { event.preventDefault(); window.location.hash = `#${views[index]}` }
        return
      }
      if (event.key === 'r' || event.key === 'R') { event.preventDefault(); action(load, '数据已刷新。', 'refresh'); return }
      if (event.key === 'Escape') setFillTarget(null)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  })

  const symbols = useMemo(() => new Map((overview?.contracts ?? []).map((item) => [item.conid, item.symbol])), [overview])
  const visibleContracts = overview?.contracts.filter((item) => `${item.symbol} ${item.conid} ${item.exchange}`.toLowerCase().includes(contractQuery.toLowerCase())) ?? []
  const filteredOrders = overview?.orders.filter((order) => orderFilter === 'ALL' || (orderFilter === 'OPEN' ? ['Submitted', 'PreSubmitted'].includes(order.status) : order.status.toUpperCase() === orderFilter)) ?? []
  const pageSize = 8
  const pageCount = Math.max(1, Math.ceil(filteredOrders.length / pageSize))
  const safeOrderPage = Math.min(orderPage, pageCount - 1)
  const pageOrders = filteredOrders.slice(safeOrderPage * pageSize, (safeOrderPage + 1) * pageSize)
  const openOrders = overview?.orders.filter((order) => ['Submitted', 'PreSubmitted'].includes(order.status)).length ?? 0
  const filledOrders = overview?.orders.filter((order) => order.status.toUpperCase() === 'FILLED').length ?? 0
  const filledQuantity = overview?.orders.reduce((total, order) => total + Number(order.filled_quantity), 0) ?? 0
  const cashTotal = overview ? Number(overview.cash.find((item) => item.currency === overview.account.currency)?.cash ?? 0) : 0
  const positionCost = overview?.positions.reduce((total, item) => total + Math.abs(Number(item.position)) * Number(item.avg_cost ?? 0), 0) ?? 0
  const latestFill = overview?.fills[0]
  const contractActivity = (overview?.contracts ?? []).map((contract) => ({
    ...contract,
    orders: overview?.orders.filter((order) => order.conid === contract.conid).length ?? 0,
    position: overview?.positions.find((item) => item.conid === contract.conid)?.position ?? '0',
  }))

  const action = async (work: () => Promise<void>, success: string, actionName = 'action') => {
    if (busyAction) return
    setBusyAction(actionName)
    try {
      await work()
      setNotice(success); setError(''); await load()
    } catch (reason) {
      if (reason instanceof ApiError && reason.status === 401) return onLogout()
      setError(reason instanceof Error ? reason.message : '操作失败')
    } finally {
      setBusyAction('')
    }
  }
  const fail = (message: string) => { setError(message); setNotice('') }
  const addContract = (event: FormEvent) => {
    event.preventDefault()
    const conid = Number(contractForm.conid)
    const symbol = contractForm.symbol.trim().toUpperCase()
    if (!Number.isInteger(conid) || conid <= 0 || !symbol) return fail('请填写有效的合约 ConID 和代码。')
    if (overview?.contracts.some((item) => item.conid === conid || item.symbol.toUpperCase() === symbol)) return fail('该合约已存在，请直接从下方选择。')
    return action(async () => { await api('trading/contracts', json({ ...contractForm, conid, symbol, sec_type: 'STK' })); setContractForm({ conid: '', symbol: '', exchange: 'SMART', currency: 'USD' }) }, '合约已添加。', 'contract')
  }
  const placeOrder = (event: FormEvent) => {
    event.preventDefault()
    const quantity = Number(orderForm.quantity)
    const needsLimit = ['LMT', 'STP_LMT'].includes(orderForm.order_type)
    const needsStop = ['STP', 'STP_LMT'].includes(orderForm.order_type)
    const price = Number(orderForm.price)
    const stopPrice = Number(orderForm.stopPrice)
    if (!orderForm.conid || !Number.isFinite(quantity) || quantity <= 0) return fail('请选择合约并填写有效数量。')
    if (needsLimit && (!Number.isFinite(price) || price <= 0)) return fail('该订单必须填写有效限价。')
    if (needsStop && (!Number.isFinite(stopPrice) || stopPrice <= 0)) return fail('该订单必须填写有效触发价。')
    return action(async () => {
      await api('trading/orders', json({
        conid: Number(orderForm.conid), side: orderForm.side, order_type: orderForm.order_type,
        quantity: orderForm.quantity, lmt_price: needsLimit ? orderForm.price : null, aux_price: needsStop ? orderForm.stopPrice : null,
      }))
    }, '订单已提交。', 'order')
  }
  const cancel = (orderId: number) => action(async () => { await api(`trading/orders/${orderId}/cancel`, { method: 'POST' }) }, '订单已撤销。', `cancel-${orderId}`)
  const submitFill = (event: FormEvent) => {
    event.preventDefault()
    const price = Number(fillPrice)
    if (fillTarget === null || !Number.isFinite(price) || price <= 0) return fail('请输入有效的成交价格。')
    return action(async () => { await api(`trading/orders/${fillTarget}/fill`, json({ price: fillPrice })); setFillTarget(null); setFillPrice('') }, '模拟成交已记账。', `fill-${fillTarget}`)
  }
  const setCash = (event: FormEvent) => {
    event.preventDefault()
    const amount = Number(cashForm.amount)
    if (!/^[a-zA-Z]{3}$/.test(cashForm.currency) || !Number.isFinite(amount) || amount <= 0) return fail('请填写有效的币种和正数金额。')
    return action(async () => { await api('trading/cash', json({ ...cashForm, currency: cashForm.currency.toUpperCase() })) }, '现金余额已更新。', 'cash')
  }
  const logout = async () => { await api('auth/logout', { method: 'POST' }).catch(() => {}); onLogout() }

  return {
    activeView, overview, error, notice,
    contractForm, setContractForm, orderForm, setOrderForm, cashForm, setCashForm,
    fillTarget, setFillTarget, fillPrice, setFillPrice,
    contractQuery, setContractQuery, orderFilter, setOrderFilter, orderPage, setOrderPage,
    expandedOrder, setExpandedOrder, busyAction, lastUpdated,
    symbols, visibleContracts, filteredOrders, pageCount, safeOrderPage, pageOrders,
    openOrders, filledOrders, filledQuantity, cashTotal, positionCost, latestFill, contractActivity,
    load, action, addContract, placeOrder, cancel, submitFill, setCash, logout,
  }
}
