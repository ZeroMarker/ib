use axum::{
    http::header,
    response::{Html, IntoResponse, Response},
};

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub async fn styles() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLES_CSS,
    )
        .into_response()
}

pub async fn app_js() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
        .into_response()
}

const INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="模拟交易平台登录与账户中心">
  <title>ib · 模拟交易平台</title>
  <link rel="stylesheet" href="styles.css">
</head>
<body>
  <main class="shell">
    <section class="hero">
      <div class="brand-mark">ib</div>
      <p class="eyebrow">PAPER TRADING PLATFORM</p>
      <h1>把策略想法，<br><em>安全地跑一遍。</em></h1>
      <p class="hero-copy">用于策略开发、纸上交易和账务演练的模拟交易平台。</p>
      <div class="hero-points">
        <span>实时账本</span><span>多空持仓</span><span>不触碰真实市场</span>
      </div>
    </section>

    <section class="auth-card" aria-label="用户认证">
      <div id="auth-view">
        <div class="tabs" role="tablist">
          <button class="tab active" data-mode="login" role="tab">登录</button>
          <button class="tab" data-mode="register" role="tab">注册</button>
        </div>
        <div class="card-heading">
          <p class="eyebrow">WELCOME BACK</p>
          <h2 id="form-title">登录账户</h2>
          <p id="form-subtitle">进入你的模拟交易空间。</p>
        </div>
        <form id="auth-form">
          <label for="email">邮箱</label>
          <input id="email" name="email" type="email" autocomplete="email" placeholder="you@example.com" required>
          <label for="password">密码</label>
          <input id="password" name="password" type="password" autocomplete="current-password" placeholder="至少 8 位" minlength="8" required>
          <button class="primary-button" type="submit" id="submit-button">登录</button>
          <p class="form-message" id="form-message" role="alert"></p>
        </form>
        <p class="legal">继续即表示你了解这是模拟交易服务，不会发送真实订单。</p>
      </div>

      <div id="dashboard-view" class="dashboard hidden">
        <div class="dashboard-top">
          <div>
            <p class="eyebrow">YOUR SIMULATION SPACE</p>
            <h2>欢迎回来</h2>
          </div>
          <button class="text-button" id="logout-button">退出登录</button>
        </div>
        <div class="user-pill"><span class="status-dot"></span><span id="user-email"></span></div>
        <div class="verify-note" id="verify-note"></div>
        <div class="stat-grid">
          <div class="stat-card"><span>账户</span><strong>模拟账户</strong><small>准备开始</small></div>
          <div class="stat-card"><span>持仓</span><strong>—</strong><small>尚未建立</small></div>
          <div class="stat-card"><span>订单</span><strong>—</strong><small>等待策略接入</small></div>
        </div>
        <p class="dashboard-note">交易 API 和账本服务已就绪，行情驱动撮合、手续费与风控模块将逐步接入。</p>
      </div>
    </section>
  </main>
  <footer>ib · simulation trading platform</footer>
  <script src="app.js"></script>
</body>
</html>"##;

const STYLES_CSS: &str = r##":root {
  color-scheme: light;
  --ink: #17211f;
  --muted: #71807b;
  --line: #dfe8e3;
  --paper: #f5f8f4;
  --card: rgba(255, 255, 255, .9);
  --green: #1d7055;
  --green-dark: #13513e;
  --lime: #d6f08a;
  --shadow: 0 24px 70px rgba(27, 59, 47, .12);
}

* { box-sizing: border-box; }
body {
  margin: 0;
  min-height: 100vh;
  color: var(--ink);
  background: var(--paper);
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}
body::before {
  position: fixed;
  inset: 0;
  z-index: -1;
  content: "";
  background:
    radial-gradient(circle at 8% 10%, rgba(214, 240, 138, .55), transparent 28rem),
    radial-gradient(circle at 90% 88%, rgba(174, 221, 201, .55), transparent 30rem);
}

.shell {
  display: grid;
  grid-template-columns: minmax(0, 1.05fr) minmax(360px, .75fr);
  gap: clamp(44px, 8vw, 130px);
  width: min(1120px, calc(100% - 48px));
  min-height: calc(100vh - 62px);
  margin: 0 auto;
  align-items: center;
}
.hero { padding: 44px 0; }
.brand-mark {
  display: grid;
  width: 54px;
  height: 54px;
  place-items: center;
  margin-bottom: 62px;
  color: var(--green-dark);
  background: var(--lime);
  border-radius: 17px;
  font-size: 25px;
  font-weight: 800;
  letter-spacing: -.09em;
}
.eyebrow {
  margin: 0 0 14px;
  color: var(--green);
  font-size: 11px;
  font-weight: 800;
  letter-spacing: .16em;
}
h1, h2, p { margin-top: 0; }
h1 {
  max-width: 680px;
  margin-bottom: 25px;
  font-size: clamp(46px, 7vw, 82px);
  line-height: .98;
  letter-spacing: -.07em;
}
h1 em { color: var(--green); font-style: normal; }
.hero-copy { max-width: 410px; color: var(--muted); font-size: 18px; line-height: 1.65; }
.hero-points { display: flex; flex-wrap: wrap; gap: 9px; margin-top: 35px; }
.hero-points span {
  padding: 8px 12px;
  color: var(--green-dark);
  background: rgba(255, 255, 255, .65);
  border: 1px solid rgba(29, 112, 85, .12);
  border-radius: 99px;
  font-size: 12px;
}

.auth-card {
  min-height: 490px;
  padding: clamp(26px, 5vw, 48px);
  background: var(--card);
  border: 1px solid rgba(255, 255, 255, .9);
  border-radius: 28px;
  box-shadow: var(--shadow);
  backdrop-filter: blur(16px);
}
.tabs { display: flex; gap: 22px; margin-bottom: 48px; border-bottom: 1px solid var(--line); }
.tab {
  position: relative;
  padding: 0 0 14px;
  color: var(--muted);
  background: transparent;
  border: 0;
  cursor: pointer;
  font: inherit;
  font-size: 14px;
}
.tab.active { color: var(--ink); font-weight: 700; }
.tab.active::after {
  position: absolute;
  right: 0;
  bottom: -1px;
  left: 0;
  height: 2px;
  content: "";
  background: var(--green);
}
.card-heading h2 { margin-bottom: 7px; font-size: 29px; letter-spacing: -.04em; }
.card-heading p:not(.eyebrow) { color: var(--muted); font-size: 14px; }
form { margin-top: 30px; }
label { display: block; margin: 18px 0 8px; font-size: 12px; font-weight: 700; }
input {
  width: 100%;
  padding: 14px 15px;
  color: var(--ink);
  background: #fff;
  border: 1px solid var(--line);
  border-radius: 11px;
  outline: none;
  font: inherit;
  transition: border-color .2s, box-shadow .2s;
}
input:focus { border-color: var(--green); box-shadow: 0 0 0 4px rgba(29, 112, 85, .1); }
.primary-button {
  width: 100%;
  margin-top: 26px;
  padding: 15px;
  color: white;
  background: var(--green);
  border: 0;
  border-radius: 11px;
  cursor: pointer;
  font: inherit;
  font-weight: 700;
  transition: transform .2s, background .2s;
}
.primary-button:hover { background: var(--green-dark); transform: translateY(-1px); }
.primary-button:disabled { cursor: wait; opacity: .6; transform: none; }
.form-message { min-height: 20px; margin: 13px 0 0; color: #a54b3e; font-size: 13px; }
.legal, .dashboard-note { margin: 25px 0 0; color: var(--muted); font-size: 11px; line-height: 1.5; }
.hidden { display: none !important; }
.dashboard-top { display: flex; justify-content: space-between; align-items: start; gap: 16px; }
.dashboard h2 { margin-bottom: 0; font-size: 31px; letter-spacing: -.05em; }
.text-button { padding: 4px 0; color: var(--green); background: none; border: 0; cursor: pointer; font: inherit; font-size: 12px; font-weight: 700; }
.user-pill { display: inline-flex; gap: 8px; align-items: center; margin-top: 29px; padding: 8px 12px; color: var(--green-dark); background: #edf7e9; border-radius: 99px; font-size: 12px; }
.status-dot { width: 7px; height: 7px; background: #5aaf62; border-radius: 50%; }
.verify-note { min-height: 18px; margin-top: 14px; color: var(--muted); font-size: 12px; }
.stat-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 9px; margin-top: 30px; }
.stat-card { min-height: 105px; padding: 13px; background: #f5f8f4; border-radius: 13px; }
.stat-card span, .stat-card small { display: block; color: var(--muted); font-size: 11px; }
.stat-card strong { display: block; margin: 13px 0 4px; font-size: 16px; }
footer { padding: 0 24px 25px; color: #9aa8a2; text-align: center; font-size: 11px; }

@media (max-width: 800px) {
  .shell { grid-template-columns: 1fr; gap: 0; width: min(540px, calc(100% - 32px)); }
  .hero { padding: 35px 0 25px; }
  .brand-mark { margin-bottom: 38px; }
  h1 { font-size: clamp(43px, 13vw, 68px); }
  .hero-copy { font-size: 16px; }
  .auth-card { min-height: 0; margin-bottom: 35px; }
}
"##;

const APP_JS: &str = r##"(() => {
  const authView = document.querySelector('#auth-view');
  const dashboardView = document.querySelector('#dashboard-view');
  const form = document.querySelector('#auth-form');
  const message = document.querySelector('#form-message');
  const title = document.querySelector('#form-title');
  const subtitle = document.querySelector('#form-subtitle');
  const submit = document.querySelector('#submit-button');
  const email = document.querySelector('#email');
  const password = document.querySelector('#password');
  let mode = 'login';

  function setMode(next) {
    mode = next;
    document.querySelectorAll('.tab').forEach((tab) => tab.classList.toggle('active', tab.dataset.mode === mode));
    title.textContent = mode === 'login' ? '登录账户' : '创建账户';
    subtitle.textContent = mode === 'login' ? '进入你的模拟交易空间。' : '开始你的纸上交易旅程。';
    submit.textContent = mode === 'login' ? '登录' : '注册';
    password.autocomplete = mode === 'login' ? 'current-password' : 'new-password';
    message.textContent = '';
  }

  function showDashboard(user) {
    authView.classList.add('hidden');
    dashboardView.classList.remove('hidden');
    document.querySelector('#user-email').textContent = user.email;
    document.querySelector('#verify-note').textContent = user.email_verified
      ? '邮箱已验证。'
      : '邮箱验证将在 Resend 邮件服务接入后开放。';
  }

  async function request(path, options = {}) {
    const response = await fetch(path, { credentials: 'same-origin', ...options });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || '请求失败，请稍后重试。');
    return data;
  }

  document.querySelectorAll('.tab').forEach((tab) => tab.addEventListener('click', () => setMode(tab.dataset.mode)));
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    message.textContent = '';
    submit.disabled = true;
    try {
      const user = await request(`api/auth/${mode}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: email.value, password: password.value }),
      });
      showDashboard(user);
    } catch (error) {
      message.textContent = error.message;
    } finally {
      submit.disabled = false;
    }
  });

  document.querySelector('#logout-button').addEventListener('click', async () => {
    await request('api/auth/logout', { method: 'POST' }).catch(() => {});
    dashboardView.classList.add('hidden');
    authView.classList.remove('hidden');
    form.reset();
    setMode('login');
  });

  request('api/auth/me').then(showDashboard).catch(() => {});
})();
"##;
