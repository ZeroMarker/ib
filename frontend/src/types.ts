/**
 * Domain types for the API payloads.
 *
 * `api-schema.json` at the repository root is the wire contract: `src/models.rs`
 * in the Rust crate is unit-tested against it, so the file cannot drift from
 * what the server actually serializes. The `assertContractMatches` helper below
 * ties the types in this file to that same file, which turns a field added or
 * renamed on the server into a compile error here instead of `undefined` at
 * runtime. That drift happened once already: `Position`, `Cash` and `Fill`
 * were strict subsets of the server payload for a long time.
 *
 * The value types are hand-written rather than derived. TypeScript widens
 * imported JSON strings to `string`, so `"lmt_price": "string|null"` cannot be
 * pattern-matched into `string | null`; only the key names survive as literals,
 * and those are what the assertions check. `api-schema.json` remains
 * authoritative for field names on both sides.
 */
import contract from '../../api-schema.json'

/** True when `Actual` and `Expected` have the same set of keys. */
type SameFields<Actual, Expected> = [Exclude<keyof Actual, keyof Expected>] extends [never]
  ? [Exclude<keyof Expected, keyof Actual>] extends [never]
    ? true
    : false
  : false

/**
 * Compile-time assertion that a hand-written type has exactly the fields the
 * contract declares, no more and no fewer.
 *
 * Returns `never` on mismatch so a drifting field breaks `tsc` at the point of
 * the assertion rather than somewhere far away in a component.
 */
/** True when `Hand` declares exactly the fields `api-schema.json` lists. */
type Matches<Name extends keyof typeof contract.types, Hand> =
  SameFields<Hand, (typeof contract.types)[Name]> extends true ? true : false

/* eslint-disable @typescript-eslint/no-unused-vars -- the assertions exist to be checked by tsc, not used at runtime. */

export type Contract = {
  conid: number
  symbol: string
  sec_type: string
  exchange: string
  currency: string
}
export type Account = {
  account_id: string
  account_type: string
  currency: string
  status: string
}
export type Order = {
  order_id: number
  perm_id: number | null
  account_id: string
  conid: number
  side: string
  order_type: string
  total_quantity: string
  filled_quantity: string
  lmt_price: string | null
  aux_price: string | null
  status: string
}
export type Position = {
  account_id: string
  conid: number
  position: string
  avg_cost: string | null
}
export type CashBalance = {
  account_id: string
  currency: string
  cash: string
}
export type Fill = {
  exec_id: string
  order_id: number
  account_id: string
  conid: number
  side: string
  quantity: string
  price: string
}
export type Overview = {
  account: Account
  contracts: Contract[]
  orders: Order[]
  positions: Position[]
  cash: CashBalance[]
  fills: Fill[]
}
export type AuthUser = { user_id: string; email: string; email_verified: boolean }

export type _ContractFieldsMatch = [
  Matches<'Contract', Contract>,
  Matches<'Account', Account>,
  Matches<'Order', Order>,
  Matches<'Position', Position>,
  Matches<'CashBalance', CashBalance>,
  Matches<'Fill', Fill>,
  Matches<'Overview', Overview>,
  Matches<'UserResponse', AuthUser>,
]

/* eslint-enable @typescript-eslint/no-unused-vars */

/**
 * The browser's install prompt, captured from `beforeinstallprompt`.
 *
 * Not in the contract: a DOM event, not a server payload.
 */
export type InstallPrompt = Event & {
  prompt: () => Promise<void>
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>
}

export type View = 'overview' | 'trade' | 'orders' | 'positions' | 'fills'

export const viewMeta: Record<View, { label: string; title: string; eyebrow: string }> = {
  overview: { label: '总览', title: '交易工作台', eyebrow: 'SIMULATION / OVERVIEW' },
  trade: { label: '交易终端', title: '提交模拟订单', eyebrow: 'SIMULATION / ORDER TICKET' },
  orders: { label: '订单管理', title: '订单管理', eyebrow: 'SIMULATION / ORDERS' },
  positions: { label: '持仓账户', title: '持仓账户', eyebrow: 'SIMULATION / POSITIONS' },
  fills: { label: '成交记录', title: '成交记录', eyebrow: 'SIMULATION / FILLS' },
}

export const viewFromHash = (): View => {
  const value = window.location.hash.slice(1) as View
  return value in viewMeta ? value : 'overview'
}
