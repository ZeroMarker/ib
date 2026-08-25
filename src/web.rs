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
        <div class="account-banner">
          <span>模拟账户</span><strong id="account-id">—</strong><small id="account-meta">加载中…</small>
        </div>
        <div class="trade-layout">
          <section class="trade-panel">
            <div class="panel-heading"><div><p class="eyebrow">PAPER ORDER</p><h3>提交模拟订单</h3></div></div>
            <form id="contract-form" class="compact-form contract-form">
              <label>新增合约</label>
              <div class="field-row"><input id="contract-conid" inputmode="numeric" placeholder="ConID" required><input id="contract-symbol" placeholder="AAPL" required></div>
              <div class="field-row"><input id="contract-exchange" value="SMART" placeholder="交易所"><input id="contract-currency" value="USD" maxlength="3" placeholder="币种"></div>
              <button class="secondary-button" type="submit">添加合约</button>
              <p class="form-message" id="contract-message" role="alert"></p>
            </form>
            <form id="order-form" class="compact-form">
              <label for="contract">合约</label>
              <select id="contract" required><option value="">请选择合约</option></select>
              <div class="field-row">
                <div><label for="side">方向</label><select id="side"><option>BUY</option><option>SELL</option></select></div>
                <div><label for="order-type">类型</label><select id="order-type"><option>MKT</option><option>LMT</option><option>STP</option><option>STP_LMT</option></select></div>
              </div>
              <div class="field-row">
                <div><label for="quantity">数量</label><input id="quantity" inputmode="decimal" value="100" required></div>
                <div><label for="price">价格</label><input id="price" inputmode="decimal" placeholder="市价可留空"></div>
              </div>
              <button class="primary-button" type="submit" id="order-submit">提交订单</button>
              <p class="form-message" id="trade-message" role="alert"></p>
            </form>
          </section>
          <section class="trade-panel">
            <div class="panel-heading"><div><p class="eyebrow">CASH LEDGER</p><h3>现金账本</h3></div></div>
            <form id="cash-form" class="compact-form">
              <div class="field-row"><div><label for="cash-currency">币种</label><input id="cash-currency" value="USD" maxlength="3" required></div><div><label for="cash-amount">余额</label><input id="cash-amount" inputmode="decimal" value="100000" required></div></div>
              <button class="secondary-button" type="submit">设置模拟余额</button>
              <p class="form-message" id="cash-message" role="alert"></p>
            </form>
            <div id="cash-list" class="mini-list"><span class="muted">暂无余额</span></div>
          </section>
        </div>
        <section class="data-panel"><div class="panel-heading"><div><p class="eyebrow">ORDERS</p><h3>订单</h3></div><button class="text-button" id="refresh-button">刷新</button></div><div class="table-wrap"><table><thead><tr><th>订单</th><th>合约</th><th>方向</th><th>数量</th><th>状态</th><th></th></tr></thead><tbody id="orders-body"><tr><td colspan="6" class="muted">暂无订单</td></tr></tbody></table></div></section>
        <div class="data-columns">
          <section class="data-panel"><div class="panel-heading"><div><p class="eyebrow">POSITIONS</p><h3>持仓</h3></div></div><div class="table-wrap"><table><thead><tr><th>合约</th><th>数量</th><th>平均成本</th></tr></thead><tbody id="positions-body"><tr><td colspan="3" class="muted">暂无持仓</td></tr></tbody></table></div></section>
          <section class="data-panel"><div class="panel-heading"><div><p class="eyebrow">FILLS</p><h3>成交</h3></div></div><div class="table-wrap"><table><thead><tr><th>执行 ID</th><th>订单</th><th>数量</th><th>价格</th></tr></thead><tbody id="fills-body"><tr><td colspan="4" class="muted">暂无成交</td></tr></tbody></table></div></section>
        </div>
        <p class="dashboard-note">当前为模拟交易：订单可撤销或注入成交，不会发送真实订单；行情撮合、手续费与风控仍未接入。</p>
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
.account-banner { display: grid; grid-template-columns: auto 1fr; gap: 4px 10px; align-items: baseline; margin-top: 27px; padding: 14px 16px; background: #f5f8f4; border-radius: 13px; }
.account-banner span, .account-banner small { color: var(--muted); font-size: 11px; }
.account-banner strong { font-size: 15px; }
.account-banner small { grid-column: 1 / -1; }
.trade-layout, .data-columns { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 10px; }
.trade-panel, .data-panel { padding: 16px; background: rgba(245, 248, 244, .8); border-radius: 15px; }
.data-panel { margin-top: 10px; }
.panel-heading { display: flex; justify-content: space-between; align-items: start; gap: 12px; margin-bottom: 12px; }
.panel-heading .eyebrow { margin-bottom: 5px; }
.panel-heading h3 { margin: 0; font-size: 16px; letter-spacing: -.03em; }
.compact-form { margin-top: 0; }
.field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 9px; }
select { width: 100%; padding: 13px 12px; color: var(--ink); background: #fff; border: 1px solid var(--line); border-radius: 11px; font: inherit; }
.compact-form label { margin-top: 12px; }
.compact-form input, .compact-form select { padding: 11px 12px; font-size: 13px; }
.compact-form .primary-button, .secondary-button { margin-top: 16px; padding: 12px; }
.secondary-button { width: 100%; color: var(--green-dark); background: var(--lime); border: 0; border-radius: 11px; cursor: pointer; font: inherit; font-size: 13px; font-weight: 700; }
.secondary-button:hover { filter: brightness(.97); }
.mini-list { margin-top: 18px; }
.mini-list div { display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid var(--line); font-size: 12px; }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 11px; white-space: nowrap; }
th { color: var(--muted); font-size: 10px; font-weight: 700; text-align: left; }
th, td { padding: 8px 5px; border-bottom: 1px solid var(--line); }
td { color: var(--ink); }
.table-action { padding: 4px 7px; color: var(--green); background: transparent; border: 1px solid var(--line); border-radius: 6px; cursor: pointer; font: inherit; font-size: 10px; }
.fill-action { margin-left: 4px; }
.muted { color: var(--muted); }
footer { padding: 0 24px 25px; color: #9aa8a2; text-align: center; font-size: 11px; }

@media (max-width: 800px) {
  .shell { grid-template-columns: 1fr; gap: 0; width: min(540px, calc(100% - 32px)); }
  .hero { padding: 35px 0 25px; }
  .brand-mark { margin-bottom: 38px; }
  h1 { font-size: clamp(43px, 13vw, 68px); }
  .hero-copy { font-size: 16px; }
  .auth-card { min-height: 0; margin-bottom: 35px; }
  .trade-layout, .data-columns { grid-template-columns: 1fr; }
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
  const contractForm = document.querySelector('#contract-form');
  const orderForm = document.querySelector('#order-form');
  const cashForm = document.querySelector('#cash-form');
  let mode = 'login';
  let snapshot = null;

  function setMode(next) {
    mode = next;
    document.querySelectorAll('.tab').forEach((tab) => tab.classList.toggle('active', tab.dataset.mode === mode));
    title.textContent = mode === 'login' ? '登录账户' : '创建账户';
    subtitle.textContent = mode === 'login' ? '进入你的模拟交易空间。' : '开始你的纸上交易旅程。';
    submit.textContent = mode === 'login' ? '登录' : '注册';
    password.autocomplete = mode === 'login' ? 'current-password' : 'new-password';
    message.textContent = '';
  }

  function escapeHtml(value) {
    return String(value ?? '').replace(/[&<>'"]/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' }[character]));
  }

  async function request(path, options = {}) {
    const response = await fetch(path, { credentials: 'same-origin', ...options });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || '请求失败，请稍后重试。');
    return data;
  }

  function showDashboard(user) {
    authView.classList.add('hidden');
    dashboardView.classList.remove('hidden');
    document.querySelector('#user-email').textContent = user.email;
    document.querySelector('#verify-note').textContent = user.email_verified
      ? '邮箱已验证。'
      : '邮箱验证将在 Resend 邮件服务接入后开放。';
    loadOverview();
  }

  function renderOverview(data) {
    snapshot = data;
    document.querySelector('#account-id').textContent = data.account.account_id;
    document.querySelector('#account-meta').textContent = `${data.account.account_type} · ${data.account.currency} · ${data.account.status}`;
    const contractSelect = document.querySelector('#contract');
    contractSelect.innerHTML = '<option value="">请选择合约</option>' + data.contracts.map((contract) =>
      `<option value="${contract.conid}">${escapeHtml(contract.symbol)} · ${escapeHtml(contract.sec_type)} · ${escapeHtml(contract.exchange)}</option>`).join('');
    document.querySelector('#cash-list').innerHTML = data.cash.length
      ? data.cash.map((item) => `<div><span>${escapeHtml(item.currency)}</span><strong>${escapeHtml(item.cash)}</strong></div>`).join('')
      : '<span class="muted">暂无余额，可在上方设置模拟余额。</span>';
    const contractName = (conid) => data.contracts.find((item) => item.conid === conid)?.symbol || `#${conid}`;
    document.querySelector('#orders-body').innerHTML = data.orders.length ? data.orders.map((order) => {
      const actions = order.status === 'Submitted' || order.status === 'PreSubmitted'
        ? `<button class="table-action" data-action="cancel" data-order="${order.order_id}">撤单</button><button class="table-action fill-action" data-action="fill" data-order="${order.order_id}">成交</button>` : '';
      return `<tr><td>#${order.order_id}</td><td>${escapeHtml(contractName(order.conid))}</td><td>${escapeHtml(order.side)}</td><td>${escapeHtml(order.total_quantity)}</td><td>${escapeHtml(order.status)}</td><td>${actions}</td></tr>`;
    }).join('') : '<tr><td colspan="6" class="muted">暂无订单</td></tr>';
    document.querySelector('#positions-body').innerHTML = data.positions.length ? data.positions.map((item) =>
      `<tr><td>${escapeHtml(contractName(item.conid))}</td><td>${escapeHtml(item.position)}</td><td>${escapeHtml(item.avg_cost || '—')}</td></tr>`).join('')
      : '<tr><td colspan="3" class="muted">暂无持仓</td></tr>';
    document.querySelector('#fills-body').innerHTML = data.fills.length ? data.fills.map((item) =>
      `<tr><td>${escapeHtml(item.exec_id)}</td><td>#${item.order_id}</td><td>${escapeHtml(item.quantity)}</td><td>${escapeHtml(item.price)}</td></tr>`).join('')
      : '<tr><td colspan="4" class="muted">暂无成交</td></tr>';
  }

  async function loadOverview() {
    try { renderOverview(await request('api/trading/overview')); }
    catch (error) { document.querySelector('#trade-message').textContent = error.message; }
  }

  document.querySelectorAll('.tab').forEach((tab) => tab.addEventListener('click', () => setMode(tab.dataset.mode)));
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    message.textContent = '';
    submit.disabled = true;
    try {
      const user = await request(`api/auth/${mode}`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: email.value, password: password.value }),
      });
      showDashboard(user);
    } catch (error) { message.textContent = error.message; }
    finally { submit.disabled = false; }
  });

  orderForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    const type = document.querySelector('#order-type').value;
    const price = document.querySelector('#price').value.trim();
    const payload = { conid: Number(document.querySelector('#contract').value), side: document.querySelector('#side').value, order_type: type, quantity: document.querySelector('#quantity').value, lmt_price: type === 'LMT' ? price || null : null, aux_price: null };
    try {
      await request('api/trading/orders', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
      document.querySelector('#trade-message').textContent = '订单已提交。';
      await loadOverview();
    } catch (error) { document.querySelector('#trade-message').textContent = error.message; }
  });

  contractForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    try {
      await request('api/trading/contracts', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({
        conid: Number(document.querySelector('#contract-conid').value), symbol: document.querySelector('#contract-symbol').value,
        sec_type: 'STK', exchange: document.querySelector('#contract-exchange').value, currency: document.querySelector('#contract-currency').value,
      }) });
      document.querySelector('#contract-message').textContent = '合约已添加。';
      contractForm.reset();
      document.querySelector('#contract-exchange').value = 'SMART';
      document.querySelector('#contract-currency').value = 'USD';
      await loadOverview();
    } catch (error) { document.querySelector('#contract-message').textContent = error.message; }
  });

  cashForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    try {
      await request('api/trading/cash', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ currency: document.querySelector('#cash-currency').value, amount: document.querySelector('#cash-amount').value }) });
      document.querySelector('#cash-message').textContent = '余额已更新。';
      await loadOverview();
    } catch (error) { document.querySelector('#cash-message').textContent = error.message; }
  });

  document.querySelector('#orders-body').addEventListener('click', async (event) => {
    const button = event.target.closest('button[data-action]');
    if (!button) return;
    const orderId = button.dataset.order;
    try {
      if (button.dataset.action === 'cancel') {
        await request(`api/trading/orders/${orderId}/cancel`, { method: 'POST' });
      } else {
        const price = window.prompt('请输入模拟成交价格');
        if (!price) return;
        await request(`api/trading/orders/${orderId}/fill`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ price }) });
      }
      await loadOverview();
    } catch (error) { document.querySelector('#trade-message').textContent = error.message; }
  });

  document.querySelector('#refresh-button').addEventListener('click', loadOverview);
  document.querySelector('#logout-button').addEventListener('click', async () => {
    await request('api/auth/logout', { method: 'POST' }).catch(() => {});
    dashboardView.classList.add('hidden'); authView.classList.remove('hidden'); form.reset(); setMode('login');
  });

  request('api/auth/me').then(showDashboard).catch(() => {});
})();
"##;
