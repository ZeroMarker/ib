import { useState, type FormEvent } from 'react'
import type { AuthUser, InstallPrompt, View } from '../types'
import { viewMeta } from '../types'
import { useTrading } from '../hooks/useTrading'
import OverviewPage from './OverviewPage'
import TradePage from './TradePage'
import OrdersPage from './OrdersPage'
import PositionsPage from './PositionsPage'
import FillsPage from './FillsPage'
import {
  Sidebar,
  DashboardHeader,
  MobileNav,
  VerifyNote,
  Alerts,
  WorkspaceNav,
  FillModal,
  DashboardFooter,
} from '../components/Layout'

type Props = {
  user: AuthUser
  onLogout: () => void
  onVerified: (user: AuthUser) => void
  installPrompt: InstallPrompt | null
  onInstalled: () => void
}

export default function Dashboard({
  user,
  onLogout,
  onVerified,
  installPrompt,
  onInstalled,
}: Props) {
  const trading = useTrading(onLogout)
  const [installing, setInstalling] = useState(false)

  const install = async () => {
    if (!installPrompt || installing) return
    setInstalling(true)
    try {
      await installPrompt.prompt()
      await installPrompt.userChoice
    } catch {
      // The user dismissed the prompt, or the browser refused it.
    } finally {
      setInstalling(false)
      onInstalled()
    }
  }

  const currentView = viewMeta[trading.activeView]
  const dataState = trading.loadFailed ? 'stale' : trading.overview ? 'online' : 'loading'
  const { overview } = trading

  const navigate = (view: View) => {
    window.location.hash = `#${view}`
  }

  const closeFillModal = () => trading.setFillTarget(null)
  const submitFill = (event: FormEvent) => trading.submitFill(event)

  return (
    <main className="dashboard-page">
      <Sidebar
        activeView={trading.activeView}
        openOrders={trading.openOrders}
        installPrompt={installPrompt}
        busyAction={trading.busyAction}
        onRefresh={trading.refresh}
        onInstall={install}
      />
      <div className="dashboard-content">
        <DashboardHeader
          email={user.email}
          currentView={currentView}
          dataState={dataState}
          installPrompt={installPrompt}
          onInstall={install}
          onLogout={trading.logout}
        />
        <MobileNav activeView={trading.activeView} />
        <VerifyNote
          email={user.email}
          emailVerified={user.email_verified}
          onVerified={onVerified}
        />
        <Alerts
          error={trading.error}
          notice={trading.notice}
          busyAction={trading.busyAction}
          onRetry={trading.refresh}
        />

        {!overview ? (
          <div className="loading-card">正在读取账户数据…</div>
        ) : (
          <>
            <WorkspaceNav
              activeView={trading.activeView}
              openOrders={trading.openOrders}
              positionsCount={overview.positions.length}
              fillsCount={overview.fills.length}
              lastUpdated={trading.lastUpdated}
              busyAction={trading.busyAction}
              onRefresh={trading.refresh}
            />
            {trading.activeView === 'overview' && (
              <OverviewPage
                overview={overview}
                dataState={dataState === 'stale' ? 'stale' : 'online'}
                openOrders={trading.openOrders}
                filledOrders={trading.filledOrders}
                filledQuantity={trading.filledQuantity}
                cashTotal={trading.cashTotal}
                positionCost={trading.positionCost}
                latestFill={trading.latestFill}
                contractActivity={trading.contractActivity}
                onNavigate={navigate}
              />
            )}
            {trading.activeView === 'trade' && (
              <TradePage
                overview={overview}
                contractForm={trading.contractForm}
                setContractForm={trading.setContractForm}
                orderForm={trading.orderForm}
                setOrderForm={trading.setOrderForm}
                cashForm={trading.cashForm}
                setCashForm={trading.setCashForm}
                contractQuery={trading.contractQuery}
                setContractQuery={trading.setContractQuery}
                visibleContracts={trading.visibleContracts}
                busyAction={trading.busyAction}
                onAddContract={trading.addContract}
                onPlaceOrder={trading.placeOrder}
                onSetCash={trading.setCash}
              />
            )}
            {trading.activeView === 'orders' && (
              <OrdersPage
                symbols={trading.symbols}
                filteredOrders={trading.filteredOrders}
                pageOrders={trading.pageOrders}
                orderFilter={trading.orderFilter}
                setOrderFilter={trading.setOrderFilter}
                safeOrderPage={trading.safeOrderPage}
                pageCount={trading.pageCount}
                setOrderPage={trading.setOrderPage}
                expandedOrder={trading.expandedOrder}
                setExpandedOrder={trading.setExpandedOrder}
                busyAction={trading.busyAction}
                onRefresh={trading.refresh}
                onCancel={trading.cancel}
                onOpenFill={trading.openFill}
              />
            )}
            {trading.activeView === 'positions' && (
              <PositionsPage positions={overview.positions} symbols={trading.symbols} />
            )}
            {trading.activeView === 'fills' && <FillsPage fills={overview.fills} />}
          </>
        )}

        <FillModal
          fillTarget={trading.fillTarget}
          fillPrice={trading.fillPrice}
          setFillPrice={trading.setFillPrice}
          busyAction={trading.busyAction}
          onClose={closeFillModal}
          onSubmit={submitFill}
        />
        <DashboardFooter lastUpdated={trading.lastUpdated} />
      </div>
    </main>
  )
}
