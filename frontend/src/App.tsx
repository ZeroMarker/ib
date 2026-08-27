import { useEffect, useState, type FormEvent } from 'react'
import Dashboard from './pages/Dashboard'
import { api, json } from './api'
import type { AuthUser, InstallPrompt } from './types'

export function App() {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [booting, setBooting] = useState(true)
  const [installPrompt, setInstallPrompt] = useState<InstallPrompt | null>(null)

  useEffect(() => {
    if ('serviceWorker' in navigator) navigator.serviceWorker.register('./sw.js?v=20260825-ux5').catch(() => {})
    const captureInstall = (event: Event) => {
      event.preventDefault()
      setInstallPrompt(event as InstallPrompt)
    }
    window.addEventListener('beforeinstallprompt', captureInstall)
    api<AuthUser>('auth/me').then(setUser).catch(() => {}).finally(() => setBooting(false))
    return () => window.removeEventListener('beforeinstallprompt', captureInstall)
  }, [])

  if (booting) return <div className="loading-screen">正在载入模拟交易空间…</div>
  return user
    ? <Dashboard user={user} onLogout={() => setUser(null)} installPrompt={installPrompt} onInstalled={() => setInstallPrompt(null)} />
    : <Auth onAuthenticated={setUser} />
}

function Auth({ onAuthenticated }: { onAuthenticated: (user: AuthUser) => void }) {
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [message, setMessage] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    setMessage('')
    setSubmitting(true)
    try {
      onAuthenticated(await api<AuthUser>(`auth/${mode}`, json({ email, password })))
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '请求失败')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <main className="shell auth-shell">
      <section className="hero">
        <div className="brand-mark">ib</div>
        <p className="eyebrow">PAPER TRADING PLATFORM</p>
        <h1>把策略想法，<br /><em>安全地跑一遍。</em></h1>
        <p className="hero-copy">用于策略开发、纸上交易和账务演练的模拟交易平台。</p>
        <div className="hero-points"><span>实时账本</span><span>多空持仓</span><span>不触碰真实市场</span></div>
      </section>
      <section className="auth-card" aria-label="用户认证">
        <div className="tabs"><button className={mode === 'login' ? 'tab active' : 'tab'} onClick={() => setMode('login')}>登录</button><button className={mode === 'register' ? 'tab active' : 'tab'} onClick={() => setMode('register')}>注册</button></div>
        <div className="card-heading"><p className="eyebrow">{mode === 'login' ? 'WELCOME BACK' : 'START SIMULATING'}</p><h2>{mode === 'login' ? '登录账户' : '创建账户'}</h2><p>{mode === 'login' ? '进入你的模拟交易空间。' : '开始你的纸上交易旅程。'}</p></div>
        <form onSubmit={submit}>
          <label htmlFor="email">邮箱</label><input id="email" type="email" autoComplete="email" placeholder="you@example.com" value={email} onChange={(event) => setEmail(event.target.value)} required />
          <label htmlFor="password">密码</label><input id="password" type="password" autoComplete={mode === 'login' ? 'current-password' : 'new-password'} placeholder="至少 8 位" minLength={8} value={password} onChange={(event) => setPassword(event.target.value)} required />
          <button className="primary-button" disabled={submitting}>{submitting ? '处理中…' : mode === 'login' ? '登录' : '注册'}</button>
          <p className="form-message">{message}</p>
        </form>
        <p className="legal">继续即表示你了解这是模拟交易服务，不会发送真实订单。</p>
      </section>
    </main>
  )
}
