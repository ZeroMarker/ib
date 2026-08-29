import type { AuthUser, InstallPrompt, Order } from '../types'
import { viewMeta } from '../types'
import { useTrading } from '../hooks/useTrading'
import OverviewPage from './OverviewPage'
import TradePage from './TradePage'
import OrdersPage from './OrdersPage'
import PositionsPage from './PositionsPage'
import FillsPage from './FillsPage'
import { Sidebar, DashboardHeader, MobileNav, VerifyNote, Alerts, WorkspaceNav, FillModal, DashboardFooter } from '../components/Layout'

export default function Dashboard({ user, onLogout, installPrompt, onInstalled }: { user: AuthUser; onLogout: () => void; installPrompt: InstallPrompt | null; onInstalled: () => void }) {
  const t = useTrading(onLogout)
  const install = async () => {
    if (!installPrompt) return
    try {
      await installPrompt.prompt()
      await installPrompt.userChoice
    } catch {
      /* 用户关闭或浏览器拒绝安装提示 */
    } finally {
      onInstalled()
    }
  }
  const refresh = t.refresh
  const currentView = viewMeta[t.activeView]
  const dataState = t.loadFailed ? 'stale' : t.overview ? 'online' : 'loading'

  return (
    <main className="dashboard-page">
      <Sidebar activeView={t.activeView} openOrders={t.openOrders} installPrompt={installPrompt} busyAction={t.busyAction} onRefresh={refresh} onInstall={install} />
      <div className="dashboard-content">
        <DashboardHeader email={user.email} currentView={currentView} dataState={dataState} installPrompt={installPrompt} onInstall={install} onLogout={t.logout} />
        <MobileNav activeView={t.activeView} />
        <VerifyNote emailVerified={user.email_verified} />
        <Alerts error={t.error} notice={t.notice} busyAction={t.busyAction} onRetry={refresh} />
        {!t.overview ? <div className="loading-card">正在读取账户数据…</div> : (
          <>
            <WorkspaceNav activeView={t.activeView} openOrders={t.openOrders} positionsCount={t.overview.positions.length} fillsCount={t.overview.fills.length} lastUpdated={t.lastUpdated} busyAction={t.busyAction} onRefresh={refresh} />
            {t.activeView === 'overview' && <OverviewPage overview={t.overview} dataState={dataState === 'stale' ? 'stale' : 'online'} openOrders={t.openOrders} filledOrders={t.filledOrders} filledQuantity={t.filledQuantity} cashTotal={t.cashTotal} positionCost={t.positionCost} latestFill={t.latestFill} contractActivity={t.contractActivity} onNavigate={(view) => { window.location.hash = `#${view}` }} />}
            {t.activeView === 'trade' && <TradePage overview={t.overview} contractForm={t.contractForm} setContractForm={t.setContractForm} orderForm={t.orderForm} setOrderForm={t.setOrderForm} cashForm={t.cashForm} setCashForm={t.setCashForm} contractQuery={t.contractQuery} setContractQuery={t.setContractQuery} visibleContracts={t.visibleContracts} busyAction={t.busyAction} onAddContract={t.addContract} onPlaceOrder={t.placeOrder} onSetCash={t.setCash} />}
            {t.activeView === 'orders' && <OrdersPage symbols={t.symbols} filteredOrders={t.filteredOrders} pageOrders={t.pageOrders} orderFilter={t.orderFilter} setOrderFilter={t.setOrderFilter} safeOrderPage={t.safeOrderPage} pageCount={t.pageCount} setOrderPage={t.setOrderPage} expandedOrder={t.expandedOrder} setExpandedOrder={t.setExpandedOrder} busyAction={t.busyAction} onRefresh={refresh} onCancel={t.cancel} onOpenFill={(order: Order) => { t.setFillTarget(order.order_id); t.setFillPrice(order.lmt_price ?? '') }} />}
            {t.activeView === 'positions' && <PositionsPage positions={t.overview.positions} symbols={t.symbols} />}
            {t.activeView === 'fills' && <FillsPage fills={t.overview.fills} />}
          </>
        )}
        <FillModal fillTarget={t.fillTarget} fillPrice={t.fillPrice} setFillPrice={t.setFillPrice} busyAction={t.busyAction} onClose={() => t.setFillTarget(null)} onSubmit={t.submitFill} />
        <DashboardFooter lastUpdated={t.lastUpdated} />
      </div>
    </main>
  )
}
