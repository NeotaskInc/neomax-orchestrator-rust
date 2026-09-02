import { api } from './api.js';
import { renderDelivery } from './delivery.js';
import { esc } from './format.js';
import { renderHistory, renderSessions, renderStatus, renderSubagents, renderUsage } from './render.js';

const state = { status: null, project: '', usageDays: 30, loaded: new Set() };

function setTheme(theme) {
  if (theme === 'light') document.documentElement.setAttribute('data-theme', 'light');
  else document.documentElement.removeAttribute('data-theme');
  document.querySelector('#theme').textContent = theme === 'light' ? 'Dark' : 'Light';
  try { localStorage.setItem('neomax-portal-theme', theme); } catch (_) {}
}

function refreshClock() {
  const element = document.querySelector('#clock');
  if (element) element.textContent = ` · ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}`;
}

async function loadStatus() {
  try {
    state.status = await api.status();
    const names = renderStatus(state.status, state.project);
    renderDelivery(state.status);
    renderProjectFilter(names);
    state.loaded.add('status');
  } catch (error) { showError(error); }
}

function renderProjectFilter(names) {
  const root = document.querySelector('#project-filter');
  if (names.length < 2) { root.innerHTML = ''; return; }
  const values = ['', ...names];
  root.innerHTML = `<span class="hint">project</span>${values.map(name => `<button class="button tiny ${state.project === name ? 'active' : ''}" data-project="${esc(name)}" type="button">${esc(name || 'all')}</button>`).join('')}`;
}

async function loadTab(name) {
  try {
    if (name === 'history') renderHistory(await api.history());
    if (name === 'sessions') renderSessions(await api.sessions());
    if (name === 'subagents') renderSubagents(await api.subagents());
    if (name === 'usage') renderUsage(await api.usage(state.usageDays));
    if (name === 'delivery') {
      if (!state.status) state.status = await api.status();
      renderDelivery(state.status);
    }
    state.loaded.add(name);
  } catch (error) { showError(error); }
}

function switchTab(name) {
  document.querySelectorAll('.tab').forEach(button => button.classList.toggle('active', button.dataset.tab === name));
  document.querySelectorAll('.tab-panel').forEach(panel => panel.classList.toggle('hidden', panel.id !== `${name}-panel`));
  loadTab(name);
}

function showError(error) {
  const root = document.querySelector('#alerts');
  root.classList.add('show');
  root.textContent = error?.message || String(error);
}

function showModal(title, body) {
  document.querySelector('#modal-title').textContent = title;
  document.querySelector('#modal-body').innerHTML = body;
  const dialog = document.querySelector('#modal');
  if (!dialog.open && typeof dialog.showModal === 'function') dialog.showModal();
  else dialog.setAttribute('open', '');
}

async function showLog(id) {
  try {
    showModal(`Run log · ${id}`, '<pre>Loading...</pre>');
    showModal(`Run log · ${id}`, `<pre>${esc(await api.log(id)) || '(no log available)'}</pre>`);
  } catch (error) { showError(error); }
}

async function showDiff(id) {
  try {
    showModal(`Run diff · ${id}`, '<pre>Loading...</pre>');
    const diff = await api.diff(id);
    showModal(`Run diff · ${id}`, `<p>${esc(diff.status || '')}</p>${diff.error ? `<p class="error">${esc(diff.error)}</p>` : ''}<pre>${esc(diff.patch || '(no patch available)')}</pre>`);
  } catch (error) { showError(error); }
}

async function showModes() {
  try {
    const response = await api.modes();
    const rows = (response.modes || []).map(mode => `<div class="project-card"><h3>${esc(mode.title || mode.id)}</h3><p class="mono">${esc(mode.cmd)}</p><p>${esc(mode.orchestrator || 'dynamic')} → ${esc(mode.workers || '')}</p></div>`).join('');
    const commands = (response.account_commands || []).map(item => `<p><strong>${esc(item.what)}</strong><br><code class="mono">${esc(item.cmd)}</code></p>`).join('');
    showModal('Quick actions', `${rows}<h3>CLI reference</h3>${commands}<p class="hint">Local account and run controls require this portal's loopback origin. Launches remain available from the Neomax CLI.</p>`);
  } catch (error) { showError(error); }
}

async function runAction(action, payload = {}, confirmed = false) {
  try {
    const response = await api.action(action, { ...payload, confirm: confirmed });
    const plan = response.plan;
    const details = plan
      ? `<p class="small-text">${esc(plan.message || '')}</p><pre>${esc([plan.program, ...(plan.args || [])].join(' '))}</pre>`
      : '';
    showModal(`Action · ${response.operation || action}`, `<p>${esc(response.message || (response.executed ? 'Action started.' : 'Action accepted.'))}</p>${details}`);
    await loadStatus();
  } catch (error) { showError(error); }
}

async function confirmAction(action, payload, label) {
  if (window.confirm(`Confirm ${label}? This affects local Neomax state.`)) await runAction(action, payload, true);
}

async function showPrState(url) {
  try {
    showModal('Pull request state', '<p>Loading...</p>');
    const response = await api.prState(url);
    const stateText = response.available
      ? `${response.state || 'unknown'}${response.isDraft ? ' · draft' : ''}${response.merged ? ' · merged' : ''}`
      : (response.error || 'state unavailable');
    showModal('Pull request state', `<p><strong>${esc(stateText)}</strong></p><p class="small-text mono">${esc(response.url || url)}</p>`);
  } catch (error) { showError(error); }
}

document.addEventListener('click', event => {
  const target = event.target.closest('button');
  if (!target) return;
  if (target.dataset.tab) switchTab(target.dataset.tab);
  else if (target.dataset.project !== undefined) {
    state.project = target.dataset.project;
    if (state.status) { const names = renderStatus(state.status, state.project); renderProjectFilter(names); }
  } else if (target.dataset.log) showLog(target.dataset.log);
  else if (target.dataset.diff) showDiff(target.dataset.diff);
  else if (target.dataset.pr) showPrState(target.dataset.pr);
  else if (target.dataset.accountAction) {
    runAction(target.dataset.accountAction, { engine: target.dataset.engine, account: target.dataset.account });
  } else if (target.dataset.runAction) {
    const action = target.dataset.runAction;
    const payload = { run_id: target.dataset.runId };
    if (target.dataset.destructive) confirmAction(action, payload, action);
    else runAction(action, payload);
  }
  else if (target.dataset.days) {
    state.usageDays = Number(target.dataset.days);
    document.querySelectorAll('[data-days]').forEach(button => button.classList.toggle('active', button === target));
    loadTab('usage');
  } else if (target.id === 'quick') showModes();
  else if (target.id === 'theme') setTheme(document.documentElement.hasAttribute('data-theme') ? 'dark' : 'light');
});

document.querySelector('#modal-close').addEventListener('click', () => document.querySelector('#modal').close?.());
document.querySelector('#modal').addEventListener('click', event => { if (event.target === event.currentTarget) event.currentTarget.close?.(); });

try { setTheme(localStorage.getItem('neomax-portal-theme') || 'dark'); } catch (_) { setTheme('dark'); }
refreshClock();
setInterval(refreshClock, 1000);
setInterval(loadStatus, 5000);
loadStatus();
loadTab('history');
