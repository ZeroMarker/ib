import type { Dispatch, FormEvent, SetStateAction } from 'react'
import { PanelTitle } from '../components'
import type { Overview } from '../types'
import type { CashForm, ContractForm, OrderForm } from '../hooks/useTrading'

export type { CashForm, ContractForm, OrderForm }

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

const ORDER_TYPES = ['MKT', 'LMT', 'STP', 'STP_LMT']
const STOP_TYPES = ['STP', 'STP_LMT']

function SubmitButton({
  busy,
  label,
  pending,
  className,
}: {
  busy: boolean
  label: string
  pending: boolean
  className: string
}) {
  return (
    <button type="submit" className={className} disabled={busy}>
      {pending ? '处理中…' : label}
    </button>
  )
}

function ContractFormPanel({
  form,
  setForm,
  busy,
  busyAction,
  onSubmit,
}: {
  form: ContractForm
  setForm: Dispatch<SetStateAction<ContractForm>>
  busy: boolean
  busyAction: string
  onSubmit: (event: FormEvent) => void
}) {
  const update = (patch: Partial<ContractForm>) => setForm({ ...form, ...patch })
  return (
    <form className="compact-form" onSubmit={onSubmit}>
      <fieldset>
        <legend>新增合约</legend>
        <div className="field-row">
          <div>
            <label htmlFor="contract-conid">ConID</label>
            <input
              id="contract-conid"
              inputMode="numeric"
              placeholder="ConID"
              value={form.conid}
              onChange={(event) => update({ conid: event.target.value })}
              required
            />
          </div>
          <div>
            <label htmlFor="contract-symbol">代码</label>
            <input
              id="contract-symbol"
              placeholder="AAPL"
              value={form.symbol}
              onChange={(event) => update({ symbol: event.target.value })}
              required
            />
          </div>
        </div>
        <div className="field-row">
          <div>
            <label htmlFor="contract-exchange">交易所</label>
            <input
              id="contract-exchange"
              value={form.exchange}
              onChange={(event) => update({ exchange: event.target.value })}
              placeholder="交易所"
            />
          </div>
          <div>
            <label htmlFor="contract-currency">币种</label>
            <input
              id="contract-currency"
              maxLength={3}
              value={form.currency}
              onChange={(event) => update({ currency: event.target.value })}
              placeholder="币种"
            />
          </div>
        </div>
        <SubmitButton
          busy={busy}
          className="secondary-button"
          label="添加合约"
          pending={busyAction === 'contract'}
        />
      </fieldset>
    </form>
  )
}

function OrderTicket({
  form,
  setForm,
  contracts,
  contractQuery,
  setContractQuery,
  busy,
  busyAction,
  onSubmit,
}: {
  form: OrderForm
  setForm: Dispatch<SetStateAction<OrderForm>>
  contracts: Overview['contracts']
  contractQuery: string
  setContractQuery: (value: string) => void
  busy: boolean
  busyAction: string
  onSubmit: (event: FormEvent) => void
}) {
  // LMT has one price field bound to `price`; STP and STP_LMT show a stop
  // field bound to `stopPrice`; STP_LMT also shows the separate limit field.
  const isMarket = form.order_type === 'MKT'
  const isLimitOnly = !STOP_TYPES.includes(form.order_type) && !isMarket
  const update = (patch: Partial<OrderForm>) => setForm({ ...form, ...patch })

  return (
    <form className="compact-form" onSubmit={onSubmit}>
      <fieldset>
        <legend>选择并提交订单</legend>
        <label htmlFor="contract-query">搜索合约</label>
        <input
          id="contract-query"
          className="search-input"
          placeholder="代码 / ConID / 交易所"
          value={contractQuery}
          onChange={(event) => setContractQuery(event.target.value)}
        />

        <label htmlFor="order-contract">合约</label>
        <select
          id="order-contract"
          value={form.conid}
          onChange={(event) => update({ conid: event.target.value })}
          required
        >
          <option value="">{contracts.length ? '请选择合约' : '没有匹配的合约'}</option>
          {contracts.map((contract) => (
            <option key={contract.conid} value={contract.conid}>
              {contract.symbol} · {contract.sec_type} · {contract.exchange}
            </option>
          ))}
        </select>

        <div className="field-row">
          <div>
            <label htmlFor="order-side">方向</label>
            <select
              id="order-side"
              value={form.side}
              onChange={(event) => update({ side: event.target.value })}
            >
              <option>BUY</option>
              <option>SELL</option>
            </select>
          </div>
          <div>
            <label htmlFor="order-type">类型</label>
            <select
              id="order-type"
              value={form.order_type}
              onChange={(event) => update({ order_type: event.target.value })}
            >
              {ORDER_TYPES.map((type) => (
                <option key={type}>{type}</option>
              ))}
            </select>
          </div>
        </div>

        <div className="field-row">
          <div>
            <label htmlFor="order-quantity">数量</label>
            <input
              id="order-quantity"
              inputMode="decimal"
              value={form.quantity}
              onChange={(event) => update({ quantity: event.target.value })}
              required
            />
          </div>
          {isMarket ? (
            <div>
              <label htmlFor="market-price">价格</label>
              <input id="market-price" value="市价" disabled />
            </div>
          ) : (
            <div>
              <label htmlFor={isLimitOnly ? 'order-price' : 'order-stop-price'}>
                {isLimitOnly ? '限价' : '触发价'}
              </label>
              <input
                id={isLimitOnly ? 'order-price' : 'order-stop-price'}
                inputMode="decimal"
                placeholder="请输入价格"
                value={isLimitOnly ? form.price : form.stopPrice}
                onChange={(event) =>
                  update(
                    isLimitOnly ? { price: event.target.value } : { stopPrice: event.target.value },
                  )
                }
                required
              />
            </div>
          )}
        </div>

        {form.order_type === 'STP_LMT' && (
          <div>
            <label htmlFor="order-limit-price">限价</label>
            <input
              id="order-limit-price"
              inputMode="decimal"
              placeholder="请输入限价"
              value={form.price}
              onChange={(event) => update({ price: event.target.value })}
              required
            />
          </div>
        )}

        <SubmitButton
          busy={busy}
          className="primary-button"
          label="提交订单"
          pending={busyAction === 'order'}
        />
      </fieldset>
    </form>
  )
}

function CashPanel({
  cash,
  setCash,
  balances,
  busy,
  busyAction,
  onSubmit,
}: {
  cash: CashForm
  setCash: Dispatch<SetStateAction<CashForm>>
  balances: Overview['cash']
  busy: boolean
  busyAction: string
  onSubmit: (event: FormEvent) => void
}) {
  const update = (patch: Partial<CashForm>) => setCash({ ...cash, ...patch })
  return (
    <section className="trade-panel" aria-label="现金账本">
      <PanelTitle eyebrow="CASH LEDGER" title="现金账本" />
      <form className="compact-form" onSubmit={onSubmit}>
        <fieldset>
          <legend>设置模拟余额</legend>
          <div className="field-row">
            <div>
              <label htmlFor="cash-currency">币种</label>
              <input
                id="cash-currency"
                maxLength={3}
                value={cash.currency}
                onChange={(event) => update({ currency: event.target.value })}
                required
              />
            </div>
            <div>
              <label htmlFor="cash-amount">余额</label>
              <input
                id="cash-amount"
                inputMode="decimal"
                value={cash.amount}
                onChange={(event) => update({ amount: event.target.value })}
                required
              />
            </div>
          </div>
          <SubmitButton
            busy={busy}
            className="secondary-button"
            label="设置模拟余额"
            pending={busyAction === 'cash'}
          />
        </fieldset>
      </form>
      <div className="mini-list" aria-label="现金余额列表">
        {balances.length ? (
          balances.map((balance) => (
            <div key={balance.currency}>
              <span>{balance.currency}</span>
              <strong>{balance.cash}</strong>
            </div>
          ))
        ) : (
          <span className="muted">暂无余额，可在上方设置模拟余额。</span>
        )}
      </div>
    </section>
  )
}

export default function TradePage({
  overview,
  contractForm,
  setContractForm,
  orderForm,
  setOrderForm,
  cashForm,
  setCashForm,
  contractQuery,
  setContractQuery,
  visibleContracts,
  busyAction,
  onAddContract,
  onPlaceOrder,
  onSetCash,
}: Props) {
  const busy = busyAction !== ''

  return (
    <>
      <div className="view-intro">
        <span className="view-number" aria-hidden="true">
          01
        </span>
        <div>
          <p className="eyebrow">ORDER TICKET</p>
          <h2>把策略想法转成一笔模拟订单</h2>
          <p>选择合约、方向和订单类型，提交后可在订单管理中追踪状态。</p>
        </div>
        <a className="quiet-link" href="#orders">
          查看订单 <span aria-hidden="true">→</span>
        </a>
      </div>

      <div id="trade" className="trade-layout">
        <section className="trade-panel" aria-label="模拟订单">
          <PanelTitle eyebrow="PAPER ORDER" title="提交模拟订单" />
          <ContractFormPanel
            form={contractForm}
            setForm={setContractForm}
            busy={busy}
            busyAction={busyAction}
            onSubmit={onAddContract}
          />
          <OrderTicket
            form={orderForm}
            setForm={setOrderForm}
            contracts={visibleContracts}
            contractQuery={contractQuery}
            setContractQuery={setContractQuery}
            busy={busy}
            busyAction={busyAction}
            onSubmit={onPlaceOrder}
          />
        </section>
        <CashPanel
          cash={cashForm}
          setCash={setCashForm}
          balances={overview.cash}
          busy={busy}
          busyAction={busyAction}
          onSubmit={onSetCash}
        />
      </div>
    </>
  )
}
