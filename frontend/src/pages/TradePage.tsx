import type { Dispatch, FormEvent, SetStateAction } from 'react'
import { PanelTitle } from '../components'
import type { Overview } from '../types'

export type ContractForm = { conid: string; symbol: string; exchange: string; currency: string }
export type OrderForm = { conid: string; side: string; order_type: string; quantity: string; price: string; stopPrice: string }
export type CashForm = { currency: string; amount: string }

type Props = {
  overview: Overview
  contractForm: ContractForm
  setContractForm: Dispatch<SetStateAction<ContractForm>>
  orderForm: OrderForm
  setOrderForm: Dispatch<SetStateAction<OrderForm>>
  cashForm: CashForm
  setCashForm: Dispatch<SetStateAction<CashForm>>
  contractQuery: string
  setContractQuery: (value: string) => void
  visibleContracts: Overview['contracts']
  busyAction: string
  onAddContract: (event: FormEvent) => void
  onPlaceOrder: (event: FormEvent) => void
  onSetCash: (event: FormEvent) => void
}

export default function TradePage({ overview, contractForm, setContractForm, orderForm, setOrderForm, cashForm, setCashForm, contractQuery, setContractQuery, visibleContracts, busyAction, onAddContract, onPlaceOrder, onSetCash }: Props) {
  return <>
    <div className="view-intro"><span className="view-number">01</span><div><p className="eyebrow">ORDER TICKET</p><h2>把策略想法转成一笔模拟订单</h2><p>选择合约、方向和订单类型，提交后可在订单管理中追踪状态。</p></div><a className="quiet-link" href="#orders">查看订单 →</a></div>
    <div id="trade" className="trade-layout">
      <section className="trade-panel"><PanelTitle eyebrow="PAPER ORDER" title="提交模拟订单" />
        <form className="compact-form" onSubmit={onAddContract}><label>新增合约</label><div className="field-row"><input inputMode="numeric" placeholder="ConID" value={contractForm.conid} onChange={(event) => setContractForm({ ...contractForm, conid: event.target.value })} required /><input placeholder="AAPL" value={contractForm.symbol} onChange={(event) => setContractForm({ ...contractForm, symbol: event.target.value })} required /></div><div className="field-row"><input value={contractForm.exchange} onChange={(event) => setContractForm({ ...contractForm, exchange: event.target.value })} placeholder="交易所" /><input value={contractForm.currency} maxLength={3} onChange={(event) => setContractForm({ ...contractForm, currency: event.target.value })} placeholder="币种" /></div><button className="secondary-button" disabled={busyAction !== ''}>{busyAction === 'contract' ? '处理中…' : '添加合约'}</button></form>
        <form className="compact-form" onSubmit={onPlaceOrder}><label>搜索并选择合约</label><input className="search-input" placeholder="代码 / ConID / 交易所" value={contractQuery} onChange={(event) => setContractQuery(event.target.value)} /><select value={orderForm.conid} onChange={(event) => setOrderForm({ ...orderForm, conid: event.target.value })} required><option value="">{visibleContracts.length ? '请选择合约' : '没有匹配的合约'}</option>{visibleContracts.map((item) => <option key={item.conid} value={item.conid}>{item.symbol} · {item.sec_type} · {item.exchange}</option>)}</select><div className="field-row"><div><label>方向</label><select value={orderForm.side} onChange={(event) => setOrderForm({ ...orderForm, side: event.target.value })}><option>BUY</option><option>SELL</option></select></div><div><label>类型</label><select value={orderForm.order_type} onChange={(event) => setOrderForm({ ...orderForm, order_type: event.target.value })}><option>MKT</option><option>LMT</option><option>STP</option><option>STP_LMT</option></select></div></div><div className="field-row"><div><label>数量</label><input inputMode="decimal" value={orderForm.quantity} onChange={(event) => setOrderForm({ ...orderForm, quantity: event.target.value })} required /></div>{orderForm.order_type === 'MKT' ? <div><label>价格</label><input value="市价" disabled /></div> : <div><label>{['STP', 'STP_LMT'].includes(orderForm.order_type) ? '触发价' : '限价'}</label><input inputMode="decimal" placeholder="请输入价格" value={['STP', 'STP_LMT'].includes(orderForm.order_type) ? orderForm.stopPrice : orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, [orderForm.order_type === 'LMT' ? 'price' : 'stopPrice']: event.target.value })} required /></div>}</div>{orderForm.order_type === 'STP_LMT' && <div><label>限价</label><input inputMode="decimal" placeholder="请输入限价" value={orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, price: event.target.value })} required /></div>}<button className="primary-button" disabled={busyAction !== ''}>{busyAction === 'order' ? '提交中…' : '提交订单'}</button></form>
      </section>
      <section className="trade-panel"><PanelTitle eyebrow="CASH LEDGER" title="现金账本" /><form className="compact-form" onSubmit={onSetCash}><div className="field-row"><div><label>币种</label><input maxLength={3} value={cashForm.currency} onChange={(event) => setCashForm({ ...cashForm, currency: event.target.value })} required /></div><div><label>余额</label><input inputMode="decimal" value={cashForm.amount} onChange={(event) => setCashForm({ ...cashForm, amount: event.target.value })} required /></div></div><button className="secondary-button" disabled={busyAction !== ''}>{busyAction === 'cash' ? '处理中…' : '设置模拟余额'}</button></form><div className="mini-list">{overview.cash.length ? overview.cash.map((item) => <div key={item.currency}><span>{item.currency}</span><strong>{item.cash}</strong></div>) : <span className="muted">暂无余额，可在上方设置模拟余额。</span>}</div></section>
    </div>
  </>
}
