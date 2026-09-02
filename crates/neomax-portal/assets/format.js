export function esc(value) {
  return String(value ?? '').replace(/[&<>"']/g, char => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  }[char]));
}

export function num(value) {
  const number = Number(value || 0);
  return Number.isFinite(number) ? number : 0;
}

export function fmtCount(value) {
  return Math.round(num(value)).toLocaleString();
}

export function fmtTokens(value) {
  const number = num(value);
  if (number < 1000) return fmtCount(number);
  if (number < 1_000_000) return `${(number / 1000).toFixed(number < 10_000 ? 1 : 0)}k`;
  return `${(number / 1_000_000).toFixed(number < 10_000_000 ? 1 : 0)}m`;
}

export function fmtUsd(value) {
  return `$${num(value).toFixed(2)}`;
}

export function fmtTime(value) {
  if (!num(value)) return '-';
  return new Date(num(value) * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

export function age(value, now = Date.now() / 1000) {
  const seconds = Math.max(0, num(now) - num(value));
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

export function shortPath(value) {
  const parts = String(value || '').split('/').filter(Boolean);
  return parts.at(-1) || '-';
}

export function shortId(value) {
  return String(value || '').slice(0, 12) || '-';
}

export function percent(value) {
  const number = Math.min(100, Math.max(0, num(value)));
  return `${number.toFixed(number % 1 ? 1 : 0)}%`;
}
