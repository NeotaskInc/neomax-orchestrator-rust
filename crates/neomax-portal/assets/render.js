import { age, esc, fmtCount, fmtTime, fmtTokens, fmtUsd, num, percent, shortId, shortPath } from './format.js';

export function renderStatus(data, selectedProject = '') {
  renderStats(data.summary || {});
  renderAlerts(data);
  renderRotationHistory(data.summary?.auth_rotations || []);
  renderEngines(data.engines || {});
  renderRuns(data.runs || [], selectedProject);
  renderAmbient(data.ambient || []);
  renderProjects(data.projects || {}, data.tasks || [], data.queue);
  return projectNames(data);
}

function renderStats(summary) {
  const fields = [
    ['live_total', 'Live agents'], ['agents_total', 'Total agents'], ['accounts_up', 'Accounts up'],
    ['running', 'Running runs'], ['inbox', 'Inbox'], ['tasks_open', 'Open tasks']
  ];
  document.querySelector('#stats').innerHTML = fields.map(([key, label]) =>
    `<div class="stat"><div class="value">${fmtCount(summary[key])}</div><div class="label">${label}</div></div>`).join('');
}

function renderAlerts(data) {
  const summary = data.summary || {};
  const messages = [];
  if (summary.rotate_advised?.length) messages.push(`${summary.rotate_advised.length} account(s) at the rotation wall; running work will hand off when quota is exhausted.`);
  if (summary.inbox) messages.push(`${summary.inbox} completed run(s) need acknowledgement.`);
  for (const item of data.errors || []) messages.push(`${item.component}: ${item.message}`);
  const element = document.querySelector('#alerts');
  element.classList.toggle('show', messages.length > 0);
  element.textContent = messages.join(' · ');
}

function renderRotationHistory(rotations) {
  const banner = document.querySelector('#rotation-banner');
  const root = document.querySelector('#rotations');
  if (!banner || !root) return;
  banner.classList.toggle('show', rotations.length > 0);
  banner.textContent = rotations.length
    ? `${rotations.length} authentication rotation${rotations.length === 1 ? '' : 's'} recorded in the last six hours. Managed work continues on eligible accounts.`
    : '';
  root.innerHTML = rotations.length
    ? rotations.map(rotation => {
      const source = rotation.source ? esc(rotation.source) : '-';
      const destination = esc(rotation.destination || 'profile');
      const transition = rotation.source ? `${source} -> ${destination}` : destination;
      const reason = rotation.reason ? ` · ${esc(rotation.reason)}` : '';
      return `<article class="rotation-entry"><div><strong>${esc(rotation.engine || 'provider')}</strong> <span class="status">${esc(rotation.operation || 'other')}</span></div><div class="mono">${transition}</div><div class="small-text">${esc(fmtTime(rotation.ts))}${reason}</div></article>`;
    }).join('')
    : '<div class="empty">No authentication rotations recorded in the last six hours.</div>';
}

function flag(value) {
  return value ? 'yes' : 'no';
}

function quotaHint(account) {
  return quotaLabel(account.capabilities?.quota || {});
}

function quotaLabel(quota) {
  if (quota.reactive) {
    return `reactive telemetry${quota.source ? ` · ${esc(quota.source)}` : ''}`;
  }
  if (quota.available) return esc(quota.source || 'numeric quota');
  if (quota.supported) return 'numeric quota unavailable';
  return 'no numeric quota';
}

function tokenValue(tokens, longName, shortName) {
  return tokens?.[longName] ?? tokens?.[shortName] ?? 0;
}

function fileStats(files = []) {
  return files.reduce((result, file) => ({
    count: result.count + 1,
    adds: result.adds + num(file?.adds),
    dels: result.dels + num(file?.dels),
  }), { count: 0, adds: 0, dels: 0 });
}

function fileDetails(files = []) {
  const stats = fileStats(files);
  const paths = files.map(file => file?.path).filter(Boolean).map(path => esc(path)).join(' · ');
  return `${fmtCount(stats.count)} files (+${fmtCount(stats.adds)} / -${fmtCount(stats.dels)})${paths ? `<div class="small-text mono">${paths}</div>` : ''}`;
}

function recordStatus(record) {
  const states = [];
  if (record.active) states.push('active');
  if (record.working) states.push('working');
  if (record.done) states.push('done');
  if (record.archived) states.push('archived');
  if (record.orchestrator) states.push('orchestrator');
  if (record.worker) states.push('worker');
  return states.join(' · ') || record.kind || 'idle';
}

function recordContext(record) {
  const lines = [
    record.cwd ? shortPath(record.cwd) : '',
    record.project ? `project ${record.project}` : '',
    record.branch ? `branch ${record.branch}` : '',
    record.workflow ? `workflow ${record.workflow}` : '',
    record.label ? `label ${record.label}` : '',
    record.slug ? `slug ${record.slug}` : '',
    record.model ? `model ${record.model}` : '',
    record.started ? `started ${fmtTime(record.started)}` : '',
    record.children?.length ? `${fmtCount(record.children.length)} child sessions` : '',
  ].filter(Boolean);
  return `${lines.map(line => esc(line)).join(' · ') || '-'}<div class="small-text">${esc(recordStatus(record))}</div>`;
}

function recordTokens(tokens) {
  return `${fmtTokens(tokenValue(tokens, 'input', 'in'))} in / ${fmtTokens(tokenValue(tokens, 'output', 'out'))} out / ${fmtTokens(tokenValue(tokens, 'reasoning', 'reasoning'))} reason<div class="small-text">cache ${fmtTokens(tokenValue(tokens, 'cache_read', 'cr'))} read / ${fmtTokens(tokenValue(tokens, 'cache_write', 'cw'))} write / total ${fmtTokens(tokenValue(tokens, 'total', 'total'))}</div>`;
}

function renderEngines(engines) {
  const root = document.querySelector('#engines');
  const names = Object.keys(engines);
  if (!names.length) { root.innerHTML = '<div class="empty">No provider profiles discovered.</div>'; return; }
  root.innerHTML = names.map(engine => {
    const accounts = engines[engine]?.accounts || [];
    const up = accounts.filter(account => account.authenticated).length;
    const capabilities = engines[engine]?.capabilities || {};
    const binary = capabilities.binary_available ? 'binary ready' : 'binary unavailable';
    return `<article class="engine"><div class="engine-head"><h3>${esc(engine)}</h3><span class="hint">${up}/${accounts.length} connected · ${binary} · ${quotaLabel(capabilities.quota || {})}</span></div>${accounts.length ? accounts.map(account => accountCard({ ...account, engine })).join('') : '<div class="empty">No accounts found.</div>'}</article>`;
  }).join('');
}

function accountCard(account) {
  const usage = account.usage || {};
  const five = usage.five_hour || {};
  const weekly = usage.seven_day || {};
  const telemetry = account.telemetry?.totals || {};
  const state = account.rotate_advised ? 'warn' : account.cooldown_until > Date.now() / 1000 ? 'cool' : account.authenticated ? 'up' : '';
  const classes = account.role === 'orchestrator' ? 'account orchestrator' : 'account';
  const chips = [account.role === 'orchestrator' ? '<span class="chip gold">orchestrator</span>' : '', account.paused ? '<span class="chip">paused</span>' : '', account.rotate_advised ? '<span class="chip warn">rotate advised</span>' : '', account.auth_method ? `<span class="chip">${esc(account.auth_method)}</span>` : ''].join(' ');
  const action = account.authenticated
    ? `<button class="action-link" data-account-action="${account.paused ? 'unpause' : 'pause'}" data-engine="${esc(account.engine || '')}" data-account="${esc(account.n)}" type="button">${account.paused ? 'unpause' : 'pause'}</button>`
    : `<button class="action-link" data-account-action="connect" data-engine="${esc(account.engine || '')}" data-account="${esc(account.n)}" type="button">connect</button>`;
  const eligibility = account.eligibility || {};
  const eligibilityLine = `reserved ${flag(account.reserved)} · credential ${flag(eligibility.credential_present)} · auth ${flag(eligibility.authenticated)} · worker ${flag(eligibility.worker_eligible)} · orchestrator ${flag(eligibility.orchestrator_eligible)} · rotation ${flag(eligibility.rotation_eligible)} · pool ${flag(eligibility.managed_pool_eligible)}`;
  const telemetryLines = [
    `tokens ${fmtTokens(telemetry.in)} in · ${fmtTokens(telemetry.out)} out · ${fmtTokens(telemetry.reasoning)} reasoning · ${fmtUsd(account.telemetry?.cost)}`,
    `cache ${fmtTokens(telemetry.cr)} read / ${fmtTokens(telemetry.cw)} write · ${fmtCount(telemetry.requests)} requests · ${fmtCount(telemetry.completions)} completions`,
    `${fmtCount(telemetry.errors)} errors · ${fmtCount(telemetry.rate_limits)} rate limits · ${fmtCount(telemetry.tool_calls)} tools / ${fmtCount(telemetry.tool_errors)} tool errors`,
    `${fmtCount(telemetry.sessions)} sessions · ${fmtCount(telemetry.main_sessions)} mains · ${fmtCount(telemetry.native_subagents)} native subagents · ${fmtCount(telemetry.files)} files (+${fmtCount(telemetry.adds)} / -${fmtCount(telemetry.dels)})`,
    telemetry.last_activity ? `last activity ${fmtTime(telemetry.last_activity)}` : 'last activity -',
  ];
  return `<div class="${classes}"><i class="dot ${state}" aria-hidden="true"></i><div><div class="name">acct ${esc(account.n)} ${chips}</div><div class="small-text">${esc(account.display_name || account.name || '')} ${account.email ? `· ${esc(account.email)}` : ''}</div><div class="small-text">${esc(eligibilityLine)}</div></div><div class="account-meta"><div>${fmtCount(account.workers)} workers · ${fmtCount(account.mains)} mains · ${fmtCount(account.subagents)} subagents</div><div class="bar-row"><span class="bar-label">5h</span><span class="bar"><i class="${num(five.used_percent) >= 99 ? 'warn' : ''}" style="width:${num(five.used_percent)}%"></i></span><span>${percent(five.used_percent)}</span></div><div class="bar-row"><span class="bar-label">7d</span><span class="bar"><i class="${num(weekly.used_percent) >= 99 ? 'warn' : ''}" style="width:${num(weekly.used_percent)}%"></i></span><span>${percent(weekly.used_percent)}</span></div>${telemetryLines.map(line => `<div class="small-text telemetry-detail">${line}</div>`).join('')}<div class="small-text quota-detail">quota: ${quotaHint(account)}</div></div><div class="account-meta">${account.cooldown_until ? `cooldown until ${esc(fmtTime(account.cooldown_until))}` : account.token_expired ? '<span class="chip warn">token expired</span>' : account.authenticated ? '<span class="chip">ready</span>' : '<span class="chip">not connected</span>'}<div>${action}</div></div></div>`;
}

function renderRuns(runs, project) {
  const visible = project ? runs.filter(run => run.project === project) : runs;
  document.querySelector('#run-count').textContent = `${visible.length} shown · ${runs.length} total`;
  const body = document.querySelector('#runs tbody');
  body.innerHTML = visible.length ? visible.map(runRow).join('') : '<tr><td class="empty" colspan="8">No runs in the current view.</td></tr>';
}

function runRow(run) {
  const runActions = [`<button class="action-link" data-log="${esc(run.id)}" type="button">log</button>`, `<button class="action-link" data-diff="${esc(run.id)}" type="button">diff</button>`];
  if (run.pr_url) runActions.push(`<button class="action-link" data-pr="${esc(run.pr_url)}" type="button">pr</button>`);
  if (['running', 'orphaned'].includes(run.status)) runActions.push(`<button class="action-link" data-run-action="kill" data-run-id="${esc(run.id)}" data-destructive="1" type="button">kill</button>`);
  if (['error', 'failed', 'orphaned'].includes(run.status)) runActions.push(`<button class="action-link" data-run-action="retry" data-run-id="${esc(run.id)}" type="button">retry</button>`);
  if (!run.acknowledged && ['done', 'completed'].includes(run.status)) runActions.push(`<button class="action-link" data-run-action="ack" data-run-id="${esc(run.id)}" type="button">ack</button>`);
  const flags = [run.effort ? `effort ${run.effort}` : '', run.ultra ? 'ultra' : '', run.opus ? 'opus' : '', run.tag ? `tag ${run.tag}` : ''].filter(Boolean).join(' · ');
  const children = (run.child_list || []).map(child => typeof child === 'string' ? child : child?.id || child?.label || '').filter(Boolean).join(' · ');
  const touchedFiles = (run.files_touched || []).map(file => esc(file)).join(' · ');
  const location = [run.worktree ? `worktree ${shortPath(run.worktree)}` : '', run.worktree_state || '', run.files_touched?.length ? `${fmtCount(run.files_touched.length)} files touched` : '', run.pr_url ? 'PR linked' : ''].filter(Boolean).join(' · ');
  const sessions = [run.session ? `session ${shortId(run.session)}` : '', run.orch_session ? `orch ${shortId(run.orch_session)}` : ''].filter(Boolean).join(' · ');
  return `<tr data-project="${esc(run.project || '')}"><td><span class="status ${esc(run.status)}">${esc(run.status)}</span>${run.acknowledged ? '<div class="small-text">acknowledged</div>' : ''}</td><td><strong>${esc(run.engine)}</strong> · acct ${esc(run.acct_no || run.account || '-')}<div class="small-text mono">${esc(shortId(run.id))}</div>${sessions ? `<div class="small-text mono">${esc(sessions)}</div>` : ''}</td><td>${esc(run.prompt || '-')}${run.goal ? `<div class="small-text">goal ${esc(run.goal)}</div>` : ''}${flags ? `<div class="small-text">${esc(flags)}</div>` : ''}</td><td>${esc(run.project || shortPath(run.repo) || '-')}${run.branch ? `<div class="small-text mono">${esc(run.branch)}</div>` : ''}${location ? `<div class="small-text">${esc(location)}</div>` : ''}${touchedFiles ? `<div class="small-text mono">files: ${touchedFiles}</div>` : ''}${children ? `<div class="small-text mono">children: ${esc(children)}</div>` : ''}</td><td class="mono">${esc(run.model || 'default')}<div class="small-text">attempt ${fmtCount(run.attempt)}</div></td><td>${fmtCount(run.children)}</td><td class="small-text">${esc(fmtTime(run.started))}${run.ended ? `<div>ended ${esc(fmtTime(run.ended))}</div>` : ''}</td><td>${runActions.join(' ')}</td></tr>`;
}

function renderAmbient(records) {
  const body = document.querySelector('#ambient tbody');
  body.innerHTML = records.length ? records.map(record => `<tr><td><strong>${esc(record.engine)}</strong> · acct ${esc(record.account || '-')}</td><td class="mono">${esc(shortId(record.id))}</td><td>${recordContext(record)}</td><td>${record.active ? '<span class="status running">active</span>' : record.working ? '<span class="status running">working</span>' : '<span class="status">idle</span>'}<div class="small-text">${esc(age(record.last_active))} ago</div></td><td>${recordTokens(record.tokens)}</td><td>${fmtCount(record.requests)} req / ${fmtCount(record.completions)} done<div class="small-text">${fmtCount(record.errors)} errors / ${fmtCount(record.rate_limits)} rate limits</div></td><td>${fmtCount(record.tool_calls)} tools / ${fmtCount(record.tool_errors)} errors<div class="small-text">${fileDetails(record.files).split('<div')[0]}</div></td><td>${fmtUsd(record.tokens?.cost)}</td></tr>`).join('') : '<tr><td class="empty" colspan="8">No interactive sessions discovered.</td></tr>';
}

export function renderHistory(records) {
  const body = document.querySelector('#history tbody');
  body.innerHTML = records.length ? records.map(run => `<tr><td><span class="status ${esc(run.status)}">${esc(run.status)}</span>${run.ultra ? '<div class="small-text">ultra</div>' : ''}${run.opus ? '<div class="small-text">opus</div>' : ''}</td><td><strong>${esc(run.engine)}</strong> · acct ${esc(run.account || '-')} ${run.account_number ? `#${esc(run.account_number)}` : ''}<div class="small-text">${run.effort ? `effort ${esc(run.effort)}` : ''}</div></td><td>${esc(run.prompt || '-')}${run.goal ? `<div class="small-text">goal ${esc(run.goal)}</div>` : ''}${run.tag ? `<div class="small-text">tag ${esc(run.tag)}</div>` : ''}</td><td class="mono">${esc(run.branch || run.repo || '-')}<div class="small-text">${fmtCount(run.children)} children · attempt ${fmtCount(run.attempt)}</div></td><td class="small-text">${esc(fmtTime(run.started))}${run.ended ? `<div>ended ${esc(fmtTime(run.ended))}</div>` : ''}</td><td><button class="action-link" data-log="${esc(run.id)}" type="button">log</button> <button class="action-link" data-diff="${esc(run.id)}" type="button">diff</button>${run.pr_url ? ` <button class="action-link" data-pr="${esc(run.pr_url)}" type="button">pr</button>` : ''}</td></tr>`).join('') : '<tr><td class="empty" colspan="6">No archived history.</td></tr>';
}

export function renderSessions(records) {
  const mains = records.filter(record => !record.parent_id && !record.kind?.includes('subagent'));
  const body = document.querySelector('#sessions tbody');
  body.innerHTML = mains.length ? mains.map(record => `<tr><td class="small-text">${esc(fmtTime(record.last_active))}<div>${esc(age(record.last_active))} ago</div></td><td>${esc(record.engine)} · acct ${esc(record.account || '-')}<div class="small-text">${esc(record.kind || 'main')}</div></td><td class="mono">${esc(shortId(record.id))}</td><td>${recordContext(record)}</td><td>${recordTokens(record.tokens)}</td><td>${fmtCount(record.requests)} req / ${fmtCount(record.completions)} done<div class="small-text">${fmtCount(record.errors)} errors / ${fmtCount(record.rate_limits)} rate limits</div></td><td>${fmtCount(record.tool_calls)} tools / ${fmtCount(record.tool_errors)} errors</td><td>${fileDetails(record.files)}</td><td>${fmtUsd(record.tokens?.cost)}</td></tr>`).join('') : '<tr><td class="empty" colspan="9">No session activity in the selected window.</td></tr>';
}

export function renderSubagents(records) {
  const body = document.querySelector('#subagents tbody');
  body.innerHTML = records.length ? records.map(record => `<tr><td class="small-text">${esc(fmtTime(record.last_active))}<div>${esc(age(record.last_active))} ago</div></td><td>${esc(record.engine)} · acct ${esc(record.account || '-')}<div class="small-text mono">parent ${esc(shortId(record.parent_id))}</div></td><td><span class="status ${record.active ? 'running' : 'done'}">${record.active ? 'working' : record.done ? 'done' : 'idle'}</span><div class="small-text">${esc(recordStatus(record))}</div></td><td>${recordContext(record)}</td><td>${recordTokens(record.tokens)}</td><td>${fmtCount(record.requests)} req / ${fmtCount(record.completions)} done<div class="small-text">${fmtCount(record.errors)} errors / ${fmtCount(record.rate_limits)} rate limits</div></td><td>${fmtCount(record.tool_calls)} tools / ${fmtCount(record.tool_errors)} errors</td><td>${fileDetails(record.files)}</td><td>${fmtUsd(record.tokens?.cost)}</td></tr>`).join('') : '<tr><td class="empty" colspan="9">No subagent activity in the selected window.</td></tr>';
}

export function renderUsage(report) {
  const grand = report?.grand || {};
  document.querySelector('#usage-summary').innerHTML = [['input', grand.in, 'Input'], ['output', grand.out, 'Output'], ['reasoning', grand.reasoning, 'Reasoning'], ['cache read', grand.cr, 'Cache read'], ['cache write', grand.cw, 'Cache write'], ['requests', grand.requests, 'Requests'], ['completions', grand.completions, 'Completions'], ['errors', grand.errors, 'Errors'], ['rate limits', grand.rate_limits, 'Rate limits'], ['cost', grand.cost, 'Estimated cost']].map(([key, value, label]) => `<div class="usage-tile"><strong>${key === 'cost' ? fmtUsd(value) : fmtTokens(value)}</strong><span>${label}</span></div>`).join('');
  const rows = report?.by_account || [];
  document.querySelector('#usage tbody').innerHTML = rows.length ? rows.map(row => usageRow(row, ['provider', 'account', 'model'], 'all models')).join('') : '<tr><td class="empty" colspan="13">No usage records in this window.</td></tr>';
  const groups = [
    ['provider', 'By provider', report?.by_provider || [], ['provider']],
    ['model', 'By model', report?.by_model || [], ['provider', 'model']],
    ['date', 'By date', report?.by_date || [], ['date']],
    ['session', 'By session', report?.by_session || [], ['provider', 'session']],
    ['agent', 'By agent', report?.by_agent || [], ['provider', 'account', 'agent']],
  ];
  const breakdowns = groups.map(([id, title, groupRows, columns]) => `<details class="usage-block" open><summary>${esc(title)} <span class="hint">${fmtCount(groupRows.length)} rows</span></summary><div class="table-scroll"><table id="usage-${id}"><thead><tr>${columns.map(column => `<th>${esc(column)}</th>`).join('')}<th>input</th><th>output</th><th>reasoning</th><th>cache read</th><th>cache write</th><th>requests</th><th>completions</th><th>errors</th><th>rate limits</th><th>cost</th></tr></thead><tbody>${groupRows.length ? groupRows.map(row => usageRow(row, columns)).join('') : `<tr><td class="empty" colspan="${columns.length + 10}">No records.</td></tr>`}</tbody></table></div></details>`).join('');
  const pricing = Object.entries(report?.pricing || {}).map(([model, price]) => `<tr><td class="mono">${esc(model)}</td><td>${fmtUsd(price.in)}</td><td>${fmtUsd(price.out)}</td><td>${fmtUsd(price.cr)}</td><td>${fmtUsd(price.cw)}</td></tr>`).join('');
  document.querySelector('#usage-breakdowns').innerHTML = `${breakdowns}<details class="usage-block"><summary>Pricing catalog <span class="hint">${fmtCount(Object.keys(report?.pricing || {}).length)} models</span></summary><div class="table-scroll"><table id="usage-pricing"><thead><tr><th>model</th><th>input / M</th><th>output / M</th><th>cache read / M</th><th>cache write / M</th></tr></thead><tbody>${pricing || '<tr><td class="empty" colspan="5">No pricing records.</td></tr>'}</tbody></table></div></details>${renderLocalDetails(report)}`;
}

function renderLocalDetails(report) {
  return [
    ['opencode', 'OpenCode SQLite detail', report?.opencode || [], 'selected range · exact local records'],
    ['kimi', 'Kimi local detail', report?.kimi || [], 'selected range · session and agent wire logs'],
    ['grok', 'Grok local detail', report?.grok || [], 'selected range · persisted session JSONL'],
  ].map(([engine, title, rows, hint]) => localProviderBlock(engine, title, rows, hint)).join('');
}

function localProviderBlock(engine, title, rows, hint) {
  if (!rows.length) return '';
  const cards = rows.map(detail => {
    const totals = detail.totals || {};
    if (!detail.available) {
      return `<div class="local-usage-card"><strong>${esc(detail.account || title)}</strong><span class="hint">${esc(detail.error || 'no local telemetry available')}</span></div>`;
    }
    const lastError = detail.last_error
      ? `<div class="small-text error">last error: ${esc(detail.last_error.name || 'Error')}${detail.last_error.status ? ` · ${esc(detail.last_error.status)}` : ''} · ${esc(detail.last_error.message || '')}${detail.last_error.at ? ` · ${esc(fmtTime(detail.last_error.at))}` : ''}</div>`
      : '';
    const readError = detail.error
      ? `<div class="small-text error">local read error: ${esc(detail.error)}</div>`
      : '';
    const modelRows = detail.models || [];
    const agentRows = detail.agents || [];
    const tools = detail.tool_usage || [];
    const toolTable = tools.length
      ? `<div class="table-scroll"><table><thead><tr><th>tool</th><th>status</th><th>calls</th></tr></thead><tbody>${tools.map(row => `<tr><td>${esc(row.tool || '?')}</td><td>${esc(row.status || '?')}</td><td>${fmtCount(row.calls)}</td></tr>`).join('')}</tbody></table></div>`
      : '<div class="empty">No local tool calls.</div>';
    const lines = [
      `${fmtTokens(totals.out)} output · ${fmtTokens(totals.in)} input · ${fmtTokens(totals.reasoning)} reasoning · ${fmtTokens(num(totals.cr) + num(totals.cw))} cache · ${fmtUsd(totals.cost)}`,
      `${fmtCount(totals.completions)}/${fmtCount(totals.requests)} completed/requests · ${fmtCount(totals.unfinished)} unfinished · ${fmtCount(totals.errors)} failures · ${fmtCount(totals.rate_limits)} rate limits`,
      `${fmtCount(totals.sessions)} sessions · ${fmtCount(totals.native_subagents)} native subagents · ${fmtCount(totals.tool_calls)} tools · ${fmtCount(totals.files)} files · +${fmtCount(totals.adds)}/-${fmtCount(totals.dels)} lines`,
      totals.last_activity ? `last ${fmtTime(totals.last_activity)}` : 'last activity -',
    ];
    const toolSection = tools.length
      ? `<details class="local-usage-section" open><summary>Tools <span class="hint">${fmtCount(totals.tool_calls)} calls</span></summary>${toolTable}</details>`
      : '';
    return `<div class="local-usage-card"><div class="local-usage-title">${esc(detail.account || title)} <span class="hint">${esc(detail.source || engine)}</span></div>${lines.map(line => `<div class="small-text">${esc(line)}</div>`).join('')}${readError}${lastError}<details class="local-usage-section" open><summary>Models <span class="hint">${fmtCount(modelRows.length)} rows</span></summary>${modelRows.length ? `<div class="table-scroll"><table>${usageHeader('model')}<tbody>${modelRows.map(row => usageRow(row, ['model'])).join('')}</tbody></table></div>` : '<div class="empty">No local model records.</div>'}</details><details class="local-usage-section" open><summary>Agents <span class="hint">${fmtCount(agentRows.length)} rows</span></summary>${agentRows.length ? `<div class="table-scroll"><table>${usageHeader('agent')}<tbody>${agentRows.map(row => usageRow(row, ['agent'])).join('')}</tbody></table></div>` : '<div class="empty">No local agent records.</div>'}</details>${toolSection}</div>`;
  }).join('');
  return `<details class="usage-block local-usage-block" open><summary>${esc(title)} <span class="hint">${esc(hint)} · ${fmtCount(rows.length)} account(s)</span></summary>${cards}</details>`;
}

function usageHeader(label) {
  return `<thead><tr><th>${esc(label)}</th><th>input</th><th>output</th><th>reasoning</th><th>cache read</th><th>cache write</th><th>requests</th><th>completions</th><th>errors</th><th>rate limits</th><th>cost</th></tr></thead>`;
}

function usageValue(row, key) {
  return row?.[key] ?? 0;
}

function usageRow(row, columns, fallbackModel = '') {
  const labels = columns.map(column => column === 'model' && fallbackModel ? fallbackModel : row?.[column] ?? '-');
  const cells = labels.map(value => `<td>${esc(value)}</td>`).join('');
  return `<tr>${cells}<td>${fmtTokens(usageValue(row, 'in'))}</td><td>${fmtTokens(usageValue(row, 'out'))}</td><td>${fmtTokens(usageValue(row, 'reasoning'))}</td><td>${fmtTokens(usageValue(row, 'cr'))}</td><td>${fmtTokens(usageValue(row, 'cw'))}</td><td>${fmtCount(usageValue(row, 'requests'))}</td><td>${fmtCount(usageValue(row, 'completions'))}</td><td>${fmtCount(usageValue(row, 'errors'))}</td><td>${fmtCount(usageValue(row, 'rate_limits'))}</td><td>${fmtUsd(usageValue(row, 'cost'))}</td></tr>`;
}

function renderProjects(projects, tasks, queue) {
  const names = Object.keys(projects);
  const reservations = queue?.queue || [];
  const body = document.querySelector('#projects');
  let cards = names.length ? names.map(name => {
    const projectTasks = tasks.filter(task => task.project === name);
    const open = projectTasks.filter(task => !['done', 'completed', 'closed', 'cancelled'].includes(task.status)).length;
    const queued = reservations.filter(item => item.task === name || item.batch === name).length;
    const project = projects[name] || {};
    const projectInfo = [
      project.root ? `root ${project.root}` : '',
      project.repos?.length ? `repos ${project.repos.join(', ')}` : '',
      project.branch_prefix ? `branch prefix ${project.branch_prefix}` : '',
      project.brain ? `brain ${project.brain}` : '',
      project.agents ? `agents ${project.agents}` : '',
      project.orch_brain ? `orchestrator ${project.orch_brain}` : '',
      project.opener ? `opener ${project.opener}` : '',
      project.planning ? `planning ${project.planning}` : '',
      project.created_at ? `created ${fmtTime(project.created_at)}` : '',
      project.auto_registered ? 'auto registered' : '',
    ].filter(Boolean);
    const taskMarkup = projectTasks.length ? projectTasks.map(task => `<div class="task-row"><strong>${esc(task.id || '-')} · ${esc(task.title || '-')}</strong><span class="status">${esc(task.status || 'todo')}</span><div class="small-text">created ${esc(fmtTime(task.created))} · updated ${esc(fmtTime(task.updated))} · runs ${esc((task.runs || []).join(', ') || '-')}</div>${task.notes?.length ? `<div class="small-text">notes: ${esc(task.notes.join(' · '))}</div>` : ''}${Object.keys(task.data || {}).length ? `<div class="small-text mono">data: ${esc(JSON.stringify(task.data))}</div>` : ''}</div>`).join('') : '<div class="empty">No tasks for this project.</div>';
    return `<article class="project-card"><h3>${esc(name)}</h3><p>${esc(project.description || project.root || 'portable project')}</p><p>${fmtCount(open)} open tasks · ${fmtCount(queued)} queued reservations</p>${projectInfo.map(info => `<p class="small-text mono">${esc(info)}</p>`).join('')}<div class="task-list">${taskMarkup}</div></article>`;
  }).join('') : '<div class="empty">No registered projects. The launch directory becomes the portable project root when registered.</div>';
  const unassigned = tasks.filter(task => !task.project);
  if (unassigned.length) {
    cards += `<article class="project-card"><h3>Unassigned tasks</h3>${unassigned.map(task => `<div class="task-row"><strong>${esc(task.id || '-')} · ${esc(task.title || '-')}</strong><span class="status">${esc(task.status || 'todo')}</span><div class="small-text">created ${esc(fmtTime(task.created))} · updated ${esc(fmtTime(task.updated))} · runs ${esc((task.runs || []).join(', ') || '-')}</div></div>`).join('')}</article>`;
  }
  const used = reservations.reduce((sum, item) => sum + Math.min(num(item.granted), num(item.want)), 0);
  const queueMarkup = queue ? `<article class="project-card queue-card"><h3>Agent queue</h3><p>${fmtCount(used)} used · ${fmtCount(Math.max(0, num(queue.agent_budget) - used))} free · ${fmtCount(queue.agent_budget)} agent budget · ${fmtCount(queue.task_budget)} task budget</p>${reservations.length ? `<div class="table-scroll"><table><thead><tr><th>id</th><th>task / batch</th><th>want / granted</th><th>session</th><th>reserved</th></tr></thead><tbody>${reservations.map(item => `<tr><td class="mono">${esc(item.id)}</td><td>${esc(item.task)}${item.batch ? `<div class="small-text">batch ${esc(item.batch)}</div>` : ''}</td><td>${fmtCount(item.want)} / ${fmtCount(item.granted)}</td><td class="mono">${esc(item.session || '-')}</td><td class="small-text">${esc(fmtTime(item.ts))}</td></tr>`).join('')}</tbody></table></div>` : '<div class="empty">No active reservations.</div>'}</article>` : '';
  body.innerHTML = `${cards}${queueMarkup}`;
}

function projectNames(data) {
  return [...new Set((data.runs || []).map(run => run.project).filter(Boolean))].sort();
}
