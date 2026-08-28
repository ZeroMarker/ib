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
  const priceId = orderForm.order_type === 'LMT' ? 'order-price' : 'order-stop-price'
  const priceLabel = ['STP', 'STP_LMT'].includes(orderForm.order_type) ? '触发价' : '限价'

  return <>
    <div className="view-intro"><span className="view-number" aria-hidden="true">01</span><div><p className="eyebrow">ORDER TICKET</p><h2>把策略想法转成一笔模拟订单</h2><p>选择合约、方向和订单类型，提交后可在订单管理中追踪状态。</p></div><a className="quiet-link" href="#orders">查看订单 <span aria-hidden="true">→</span></a></div>
    <div id="trade" className="trade-layout">
      <section className="trade-panel" aria-label="模拟订单">
        <PanelTitle eyebrow="PAPER ORDER" title="提交模拟订单" />
        <form className="compact-form" onSubmit={onAddContract}>
          <fieldset>
            <legend>新增合约</legend>
            <div className="field-row">
              <div><label htmlFor="contract-conid">ConID</label><input id="contract-conid" inputMode="numeric" placeholder="ConID" value={contractForm.conid} onChange={(event) => setContractForm({ ...contractForm, conid: event.target.value })} required /></div>
              <div><label htmlFor="contract-symbol">代码</label><input id="contract-symbol" placeholder="AAPL" value={contractForm.symbol} onChange={(event) => setContractForm({ ...contractForm, symbol: event.target.value })} required /></div>
            </div>
            <div className="field-row">
              <div><label htmlFor="contract-exchange">交易所</label><input id="contract-exchange" value={contractForm.exchange} onChange={(event) => setContractForm({ ...contractForm, exchange: event.target.value })} placeholder="交易所" /></div>
              <div><label htmlFor="contract-currency">币种</label><input id="contract-currency" value={contractForm.currency} maxLength={3} onChange={(event) => setContractForm({ ...contractForm, currency: event.target.value })} placeholder="币种" /></div>
            </div>
            <button type="submit" className="secondary-button" disabled={busyAction !== ''}>{busyAction === 'contract' ? '处理中…' : '添加合约'}</button>
          </fieldset>
        </form>
        <form className="compact-form" onSubmit={onPlaceOrder}>
          <fieldset>
            <legend>选择并提交订单</legend>
            <label htmlFor="contract-query">搜索合约</label>
            <input id="contract-query" className="search-input" placeholder="代码 / ConID / 交易所" value={contractQuery} onChange={(event) => setContractQuery(event.target.value)} />
            <label htmlFor="order-contract">合约</label>
            <select id="order-contract" value={orderForm.conid} onChange={(event) => setOrderForm({ ...orderForm, conid: event.target.value })} required><option value="">{visibleContracts.length ? '请选择合约' : '没有匹配的合约'}</option>{visibleContracts.map((item) => <option key={item.conid} value={item.conid}>{item.symbol} · {item.sec_type} · {item.exchange}</option>)}</select>
            <div className="field-row">
              <div><label htmlFor="order-side">方向</label><select id="order-side" value={orderForm.side} onChange={(event) => setOrderForm({ ...orderForm, side: event.target.value })}><option>BUY</option><option>SELL</option></select></div>
              <div><label htmlFor="order-type">类型</label><select id="order-type" value={orderForm.order_type} onChange={(event) => setOrderForm({ ...orderForm, order_type: event.target.value })}><option>MKT</option><option>LMT</option><option>STP</option><option>STP_LMT</option></select></div>
            </div>
            <div className="field-row">
              <div><label htmlFor="order-quantity">数量</label><input id="order-quantity" inputMode="decimal" value={orderForm.quantity} onChange={(event) => setOrderForm({ ...orderForm, quantity: event.target.value })} required /></div>
              {orderForm.order_type === 'MKT' ? <div><label htmlFor="market-price">价格</label><input id="market-price" value="市价" disabled /></div> : <div><label htmlFor={priceId}>{priceLabel}</label><input id={priceId} inputMode="decimal" placeholder="请输入价格" value={['STP', 'STP_LMT'].includes(orderForm.order_type) ? orderForm.stopPrice : orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, [orderForm.order_type === 'LMT' ? 'price' : 'stopPrice']: event.target.value })} required /></div>}
            </div>
            {orderForm.order_type === 'STP_LMT' && <div><label htmlFor="order-price">限价</label><input id="order-price" inputMode="decimal" placeholder="请输入限价" value={orderForm.price} onChange={(event) => setOrderForm({ ...orderForm, price: event.target.value })} required /></div>}
            <button type="submit" className="primary-button" disabled={busyAction !== ''}>{busyAction === 'order' ? '提交中…' : '提交订单'}</button>
          </fieldset>
        </form>
      </section>
      <section className="trade-panel" aria-label="现金账本">
        <PanelTitle eyebrow="CASH LEDGER" title="现金账本" />
        <form className="compact-form" onSubmit={onSetCash}>
          <fieldset>
            <legend>设置模拟余额</legend>
            <div className="field-row">
              <div><label htmlFor="cash-currency">币种</label><input id="cash-currency" maxLength={3} value={cashForm.currency} onChange={(event) => setCashForm({ ...cashForm, currency: event.target.value })} required /></div>
              <div><label htmlFor="cash-amount">余额</label><input id="cash-amount" inputMode="decimal" value={cashForm.amount} onChange={(event) => setCashForm({ ...cashForm, amount: event.target.value })} required /></div>
            </div>
            <button type="submit" className="secondary-button" disabled={busyAction !== ''}>{busyAction === 'cash' ? '处理中…' : '设置模拟余额'}</button>
          </fieldset>
        </form>
        <div className="mini-list" aria-label="现金余额列表">{overview.cash.length ? overview.cash.map((item) => <div key={item.currency}><span>{item.currency}</span><strong>{item.cash}</strong></div>) : <span className="muted">暂无余额，可在上方设置模拟余额。</span>}</div>
      </section>
    </div>
  </>
}
