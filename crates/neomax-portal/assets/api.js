async function read(path, options = {}) {
  const response = await fetch(path, { cache: 'no-store', ...options });
  const type = response.headers.get('content-type') || '';
  const body = type.includes('json') ? await response.json() : await response.text();
  if (!response.ok) throw new Error(body?.error || body || `request failed (${response.status})`);
  return body;
}

export const api = {
  status: () => read('/api/status'),
  history: (limit = 60) => read(`/api/history?limit=${limit}`),
  modes: () => read('/api/modes'),
  usage: (days = 30) => read(`/api/usage?days=${days}`),
  sessions: (days = 3) => read(`/api/sessions?days=${days}`),
  subagents: (days = 3) => read(`/api/subagents?days=${days}`),
  plans: () => read('/api/plans'),
  issues: () => read('/api/issues'),
  worktrees: () => read('/api/worktrees'),
  diff: id => read(`/api/rundiff/${encodeURIComponent(id)}`),
  log: id => read(`/api/log/${encodeURIComponent(id)}`),
  prState: url => read(`/api/prstate?url=${encodeURIComponent(url)}`),
  action: (action, payload = {}) => read('/api/action', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action, ...payload }),
  }),
  connect: (engine, account) => api.action('connect', { engine, account }),
  pause: (engine, account, paused) => api.action(paused ? 'pause' : 'unpause', { engine, account }),
  runAction: (action, runId, confirm = false) => api.action(action, { run_id: runId, confirm }),
};
