export type AuthUser = { user_id: string; email: string; email_verified: boolean }
export type Contract = { conid: number; symbol: string; sec_type: string; exchange: string; currency: string }
export type Order = { order_id: number; account_id: string; conid: number; side: string; order_type: string; total_quantity: string; filled_quantity: string; status: string; lmt_price: string | null; aux_price: string | null }
export type Position = { conid: number; position: string; avg_cost: string | null }
export type Cash = { currency: string; cash: string }
export type Fill = { exec_id: string; order_id: number; quantity: string; price: string }
export type Overview = { account: { account_id: string; account_type: string; currency: string; status: string }; contracts: Contract[]; orders: Order[]; positions: Position[]; cash: Cash[]; fills: Fill[] }
export type InstallPrompt = Event & { prompt: () => Promise<void>; userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }> }
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
