import { esc } from './format.js';

export function renderDelivery(snapshot) {
  renderPlans(snapshot?.plans || []);
  renderIssues(snapshot?.issues || []);
  renderFailovers(snapshot?.summary?.failover_events || []);
  renderWorktrees(snapshot?.worktrees || []);
}

function renderPlans(plans) {
  const root = document.querySelector('#plans');
  if (!root) return;
  root.innerHTML = plans.length
    ? plans.map(plan => {
      const definition = plan.plan && typeof plan.plan === 'object' ? plan.plan : {};
      const parts = Array.isArray(definition.parts) ? definition.parts : [];
      const state = plan.state || {};
      const states = state.states && typeof state.states === 'object' ? Object.values(state.states) : [];
      const repository = plan.repository || plan.repo || definition.repository || definition.repo || '';
      return `<article class="delivery-card"><h3>${esc(plan.plan_id || 'plan')}</h3><p class="small-text">${esc(plan.status || 'unknown')} · ${parts.length} part(s) · ${states.length} state row(s)</p><p class="small-text">${esc(repository)}</p></article>`;
    }).join('')
    : '<div class="empty">No scheduler plans recorded.</div>';
}

function renderIssues(issues) {
  const root = document.querySelector('#issues');
  if (!root) return;
  root.innerHTML = issues.length
    ? issues.map(issue => `<article class="delivery-card"><h3>${esc(issue.key || 'issue')} <span class="status">${esc(issue.status || 'unknown')}</span></h3><p>${esc(issue.title || '')}</p><p class="small-text">${esc(issue.project || '')} · updated ${esc(String(issue.updated || '-'))}</p></article>`).join('')
    : '<div class="empty">No cross-repository issues recorded.</div>';
}

function renderWorktrees(worktrees) {
  const root = document.querySelector('#worktrees tbody');
  if (!root) return;
  root.innerHTML = worktrees.length
    ? worktrees.map(worktree => `<tr><td class="mono">${esc(worktree.id || '-')}</td><td class="mono">${esc(worktree.path || '-')}</td><td>${worktree.exists ? 'present' : 'missing'}</td><td>${esc(worktree.status || worktree.state || 'unowned')}</td><td>${esc(worktree.run_id || '-')}</td><td>${esc(worktree.branch || '-')}</td></tr>`).join('')
    : '<tr><td class="empty" colspan="6">No managed worktrees recorded.</td></tr>';
}

function renderFailovers(events) {
  const root = document.querySelector('#failovers');
  if (!root) return;
  root.innerHTML = events.length
    ? events.map(event => `<article class="delivery-card"><h3>${esc(event.event || 'event')} <span class="status">${esc(event.engine || '')}</span></h3><p class="small-text">run ${esc(event.run || '-')} · account ${esc(event.account || '-')} · attempt ${esc(event.attempt || '-')}</p><p class="small-text">${esc(event.reason || event.strategy || '')}</p></article>`).join('')
    : '<div class="empty">No failover events recorded in the selected retention window.</div>';
}
