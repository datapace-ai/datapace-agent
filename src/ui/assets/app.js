// Datapace Agent — Multi-DB Control Plane UI
'use strict';

const API = '';
let databases = [];
let editingId = null;
let refreshTimer = null;
let currentView = null;       // null = list, { id: "..." } = detail page
let selectedCollector = null;  // which collector is expanded on detail page
let activeLogTab = 'pipeline'; // 'pipeline' or 'shipping'

// === Helpers ===

async function fetchJSON(path, opts) {
  const r = await fetch(API + path, opts);
  if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
  return r.json();
}

function $(sel) { return document.querySelector(sel); }
function $$(sel) { return [...document.querySelectorAll(sel)]; }

function fmtNum(n) {
  if (n == null) return '-';
  if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K';
  return String(n);
}

function fmtMs(ms) {
  if (ms == null) return '-';
  if (ms >= 60000) return (ms / 60000).toFixed(1) + 'm';
  if (ms >= 1000) return (ms / 1000).toFixed(1) + 's';
  return ms + 'ms';
}

function fmtBytes(b) {
  if (!b) return '0 B';
  if (b >= 1e6) return (b / 1e6).toFixed(1) + ' MB';
  if (b >= 1e3) return (b / 1e3).toFixed(1) + ' KB';
  return b + ' B';
}

function timeAgo(iso) {
  if (!iso) return 'never';
  const diff = (Date.now() - new Date(iso).getTime()) / 1000;
  if (diff < 60) return Math.round(diff) + 's ago';
  if (diff < 3600) return Math.round(diff / 60) + 'm ago';
  if (diff < 86400) return Math.round(diff / 3600) + 'h ago';
  return Math.round(diff / 86400) + 'd ago';
}

function fmtTime(iso) {
  if (!iso) return '-';
  return new Date(iso).toLocaleTimeString();
}

function escHtml(s) {
  return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

const ENV_LABELS = { production: 'PROD', staging: 'STG', development: 'DEV', local: 'LOCAL' };

// Collectors per database type
const COLLECTORS_BY_TYPE = {
  postgres: [
    { name: 'statements', checked: true },
    { name: 'activity', checked: true },
    { name: 'locks', checked: true },
    { name: 'explain', checked: false },
    { name: 'tables', checked: true },
    { name: 'schema', checked: true },
    { name: 'io', checked: false },
  ],
  mongodb: [
    { name: 'mongo_server_status', checked: true },
    { name: 'mongo_current_ops', checked: true },
    { name: 'mongo_slow_queries', checked: true },
    { name: 'mongo_top', checked: true },
    { name: 'mongo_collections', checked: true },
    { name: 'mongo_repl_status', checked: true },
  ],
};

const URL_PLACEHOLDERS = {
  postgres: 'postgres://user:pass@host:5432/dbname',
  mongodb: 'mongodb+srv://user:pass@cluster.example.net/dbname',
};

// === SVG Icons ===

const ICON = {
  arrow: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg>`,
  back: `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2L4 7l5 5"/></svg>`,
  plus: `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M7 2v10M2 7h10"/></svg>`,
  x: `<svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2 2l8 8M10 2L2 10"/></svg>`,
  empty: `<svg width="48" height="48" viewBox="0 0 48 48" fill="none"><circle cx="24" cy="24" r="20" stroke="rgba(255,255,255,0.1)" stroke-width="1.5" stroke-dasharray="4 4"/><path d="M24 16v16M16 24h16" stroke="rgba(255,255,255,0.15)" stroke-width="1.5" stroke-linecap="round"/></svg>`,
  webhook: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M18 3l-8 8"/><path d="M18 3l-5 15-3-7-7-3 15-5z"/></svg>`,
  custom: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 12l-2 2 2 2"/><path d="M12 8l2-2-2-2"/><path d="M7 3h6a4 4 0 014 4v6a4 4 0 01-4 4H7a4 4 0 01-4-4V7a4 4 0 014-4z"/></svg>`,
  datapace: `<img src="/logo.svg" style="width:100%;height:100%;object-fit:contain">`,
};

function shipperIcon(type_) {
  const icons = { datapace: ICON.datapace, webhook: ICON.webhook, custom: ICON.custom };
  return icons[type_] || ICON.custom;
}

// === Routing ===

function navigate(hash) {
  if (location.hash !== hash) location.hash = hash;
  else route();
}

function route() {
  const hash = location.hash;
  const match = hash.match(/^#db\/(.+)$/);
  if (match) {
    currentView = { id: match[1] };
    selectedCollector = null;
    renderDetailPage();
  } else {
    currentView = null;
    selectedCollector = null;
    renderListPage();
  }
}

// === Data Loading ===

async function loadDatabases() {
  try {
    databases = await fetchJSON('/api/databases');
    if (currentView) {
      updateDetailPage();
    } else {
      renderListPage();
    }
  } catch (e) {
    console.error('Failed to load databases:', e);
  }
}

function startRefresh() {
  if (refreshTimer) clearInterval(refreshTimer);
  refreshTimer = setInterval(loadDatabases, 10000);
}

// =====================================
//  LIST PAGE
// =====================================

function renderListPage() {
  const main = $('#main-content');
  const headerBtn = $('#header-action');
  headerBtn.innerHTML = `<button class="btn-primary" onclick="openDrawer()">${ICON.plus} Add Database</button>`;

  if (databases.length === 0) {
    main.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">${ICON.empty}</div>
        <h2>No databases configured</h2>
        <p>Add a PostgreSQL or MongoDB database to start collecting metrics.</p>
        <button class="btn-primary" onclick="openDrawer()">${ICON.plus} Add Database</button>
      </div>`;
    return;
  }

  let html = '<div class="page-header"><h1 class="page-title">Databases</h1></div>';
  html += '<div class="db-list">';

  for (const db of databases) {
    const rs = db.runtime_status;
    const status = rs?.status || db.status || 'stopped';
    const stats = rs?.collector_stats || [];
    const dbType = db.db_type || 'postgres';
    const env = db.environment || 'production';
    const envLabel = ENV_LABELS[env] || env.toUpperCase();
    const totalRows = stats.reduce((sum, s) => sum + (s.rows || 0), 0);

    html += `<div class="db-row" onclick="navigate('#db/${db.id}')">`;
    html += '<div class="db-row-left">';
    html += `<div class="status-dot ${status}"></div>`;
    html += `<span class="db-name">${escHtml(db.name)}</span>`;
    html += `<span class="db-url">${escHtml(db.masked_url)}</span>`;
    html += '</div>';
    html += '<div class="db-row-meta">';
    html += `<span class="badge ${dbType}">${dbType}</span>`;
    html += `<span class="badge ${env}">${envLabel}</span>`;
    html += `<span class="db-stats">${stats.length} collectors &middot; ${fmtNum(totalRows)} rows</span>`;
    html += `<span class="db-stats">${timeAgo(rs?.last_tick)}</span>`;
    html += `<button class="btn-ghost" onclick="event.stopPropagation(); editDatabase('${db.id}')">Edit</button>`;
    html += `<span class="db-row-arrow">${ICON.arrow}</span>`;
    html += '</div>';
    html += '</div>';
  }

  html += '</div>';
  main.innerHTML = html;
}

// =====================================
//  DETAIL PAGE
// =====================================

function renderDetailPage() {
  const db = databases.find(d => d.id === currentView.id);
  const main = $('#main-content');
  const headerBtn = $('#header-action');
  headerBtn.innerHTML = '';

  if (!db) {
    main.innerHTML = '<div class="empty-state"><h2>Database not found</h2><p>It may have been deleted.</p></div>';
    return;
  }

  const rs = db.runtime_status;
  const status = rs?.status || db.status || 'stopped';
  const stats = rs?.collector_stats || [];
  const shippers = db.shippers || [];
  const shipperStatuses = rs?.shipper_statuses || [];
  const dbType = db.db_type || 'postgres';
  const env = db.environment || 'production';
  const envLabel = ENV_LABELS[env] || env.toUpperCase();
  const totalRows = stats.reduce((sum, s) => sum + (s.rows || 0), 0);
  const totalDur = stats.reduce((sum, s) => sum + (s.duration_ms || 0), 0);

  let html = '';

  // Back link
  html += `<a class="detail-back" onclick="navigate('#')">${ICON.back} All Databases</a>`;

  // Header
  html += '<div class="detail-page-header">';
  html += `<div class="status-dot ${status}"></div>`;
  html += `<h1 class="detail-page-title">${escHtml(db.name)}</h1>`;
  html += `<span class="badge ${dbType}">${dbType}</span>`;
  html += `<span class="badge ${env}">${envLabel}</span>`;
  if (db.anonymize) html += `<span class="badge" style="background:var(--green-dim);color:var(--green)">ANON</span>`;
  html += '<div class="detail-actions">';
  html += `<button class="btn-ghost" onclick="editDatabase('${db.id}')">Edit</button>`;
  html += '</div>';
  html += '</div>';
  html += `<div class="detail-page-url">${escHtml(db.masked_url)}</div>`;

  // Status bar
  html += '<div class="section">';
  html += '<div class="status-bar">';
  html += `<div class="status-item"><span class="status-label">Status</span><span class="status-value ${status}">${status}</span></div>`;
  html += `<div class="status-item"><span class="status-label">Collectors</span><span class="status-value">${stats.length}</span></div>`;
  html += `<div class="status-item"><span class="status-label">Total rows</span><span class="status-value">${fmtNum(totalRows)}</span></div>`;
  html += `<div class="status-item"><span class="status-label">Duration</span><span class="status-value">${fmtMs(totalDur)}</span></div>`;
  html += `<div class="status-item"><span class="status-label">Last tick</span><span class="status-value">${timeAgo(rs?.last_tick)}</span></div>`;
  html += `<div class="status-item"><span class="status-label">Shippers</span><span class="status-value">${shippers.length}</span></div>`;
  html += '</div>';
  html += '</div>';

  // Collectors section
  html += '<div class="section">';
  html += '<div class="section-header"><span class="section-title">Collectors</span></div>';
  html += '<div class="section-content">';
  if (db.collectors && db.collectors.length > 0) {
    html += '<div id="collector-list">';
    for (const name of db.collectors) {
      const stat = stats.find(s => s.name === name);
      const hasErr = stat?.error;
      const dotCls = hasErr ? 'err' : (stat ? 'ok' : 'off');
      const isActive = selectedCollector === name;
      html += `<div class="collector-row ${isActive ? 'active' : ''}" onclick="toggleCollector('${escHtml(name)}')">`;
      html += `<div class="collector-status-dot ${dotCls}"></div>`;
      html += `<span class="collector-name">${escHtml(name)}</span>`;
      if (stat) {
        html += `<span class="collector-stat">${fmtNum(stat.rows)} rows &middot; ${fmtMs(stat.duration_ms)}</span>`;
        if (hasErr) html += `<span class="collector-err">${escHtml(stat.error)}</span>`;
      } else {
        html += '<span class="collector-stat">no data</span>';
      }
      html += '</div>';
      if (isActive) {
        html += '<div id="collector-data-panel" class="data-panel"><div style="padding:0.5rem 0;color:var(--text-muted)">Loading...</div></div>';
      }
    }
    html += '</div>';
  } else {
    html += '<div class="empty-row">No collectors configured</div>';
  }
  html += '</div>';
  html += '</div>';

  // Shippers section
  html += '<div class="section">';
  html += '<div class="section-header">';
  html += '<span class="section-title">Shippers</span>';
  html += `<button class="btn-secondary btn-sm" onclick="openShipperModal('${db.id}', 'webhook')">${ICON.plus} Add Shipper</button>`;
  html += '</div>';
  html += '<div class="section-content" id="shipper-list">';
  if (shippers.length > 0) {
    for (const shipper of shippers) {
      const ss = shipperStatuses.find(s => s.shipper_id === shipper.id);
      const sStatus = ss ? ss.status : 'off';
      const ep = shipper.endpoint.length > 40 ? shipper.endpoint.substring(0, 40) + '...' : shipper.endpoint;
      html += '<div class="shipper-row">';
      html += '<div class="shipper-info">';
      html += `<div class="shipper-name">${escHtml(shipper.name)}</div>`;
      html += `<div class="shipper-endpoint">${escHtml(ep)}</div>`;
      html += '</div>';
      html += `<span class="badge ${shipper.shipper_type}">${escHtml(shipper.shipper_type)}</span>`;
      html += `<span class="shipper-status ${sStatus}">${sStatus === 'off' ? 'idle' : sStatus}</span>`;
      if (ss) html += `<span class="shipper-meta">${fmtBytes(ss.bytes)} &middot; ${timeAgo(ss.at)}</span>`;
      html += `<button class="shipper-remove" onclick="event.stopPropagation(); removeShipper('${db.id}', '${shipper.id}')" title="Remove">${ICON.x}</button>`;
      html += '</div>';
    }
  } else {
    html += '<div class="empty-row">No shippers configured</div>';
  }
  html += '</div>';
  html += '</div>';

  // Logs section
  html += '<div class="section">';
  html += '<div class="section-header"><span class="section-title">Logs</span></div>';
  html += '<div class="section-content">';
  html += '<div class="tabs">';
  html += `<button class="tab ${activeLogTab === 'pipeline' ? 'active' : ''}" onclick="switchLogTab('pipeline')">Pipeline</button>`;
  html += `<button class="tab ${activeLogTab === 'shipping' ? 'active' : ''}" onclick="switchLogTab('shipping')">Shipping</button>`;
  html += '</div>';
  html += '<div id="log-content"><div class="empty-row">Loading...</div></div>';
  html += '</div>';
  html += '</div>';

  main.innerHTML = html;

  // Load collector data if one was selected
  if (selectedCollector) {
    loadCollectorData(selectedCollector);
  }

  // Load logs
  loadLogTab();
}

function updateDetailPage() {
  if (!currentView) return;
  // Full re-render to update status bar, collector stats, shipper statuses
  renderDetailPage();
}

// === Collector expand/collapse ===

function toggleCollector(name) {
  if (selectedCollector === name) {
    selectedCollector = null;
    renderDetailPage();
  } else {
    selectedCollector = name;
    renderDetailPage();
  }
}

async function loadCollectorData(name) {
  if (!currentView) return;
  const panel = $('#collector-data-panel');
  if (!panel) return;

  try {
    const data = await fetchJSON(`/api/databases/${currentView.id}/collectors/${name}`);
    renderCollectorDataPanel(panel, name, data.snapshot);
  } catch (e) {
    panel.innerHTML = `<div style="color:var(--red)">Error: ${escHtml(e.message)}</div>`;
  }
}

function renderCollectorDataPanel(el, name, snapshot) {
  if (!snapshot || !snapshot.data) {
    el.innerHTML = '<div style="color:var(--text-muted)">No data yet</div>';
    return;
  }

  const rows = Array.isArray(snapshot.data) ? snapshot.data : [snapshot.data];
  if (rows.length === 0) {
    el.innerHTML = '<div style="color:var(--text-muted)">Empty</div>';
    return;
  }

  const db = databases.find(d => d.id === currentView.id);
  const isAnon = db?.anonymize;
  const anonInfo = isAnon ? countAnonTokens(rows) : { total: 0, counts: {} };

  const ts = snapshot.collected_at ? timeAgo(snapshot.collected_at) : '';
  let html = '<div class="data-panel-header">';
  html += `<span class="data-panel-meta">${rows.length} rows &middot; ${ts}</span>`;
  if (isAnon && anonInfo.total > 0) {
    const breakdown = Object.entries(anonInfo.counts).map(([k, v]) => `${v} ${k}`).join(', ');
    html += `<span class="anon-summary" title="${escHtml(breakdown)}">`;
    html += `<svg viewBox="0 0 16 16" width="13" height="13"><path d="M8 1L2 4v4c0 3.5 2.6 6.4 6 7 3.4-.6 6-3.5 6-7V4L8 1z" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.5 8l2 2 3.5-4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
    html += ` ${anonInfo.total} anonymized</span>`;
  }
  html += '</div>';

  const keys = Object.keys(rows[0]);
  html += '<div class="data-scroll"><table class="data-table"><thead><tr>';
  for (const k of keys) html += `<th>${escHtml(k)}</th>`;
  html += '</tr></thead><tbody>';

  for (const row of rows.slice(0, 200)) {
    html += '<tr>';
    for (const k of keys) {
      let val = row[k];
      if (val === null || val === undefined) val = '-';
      else if (typeof val === 'number' && !Number.isInteger(val)) val = val.toFixed(2);
      else if (typeof val === 'object') val = JSON.stringify(val);
      const full = String(val);
      const display = full.length > 100 ? full.substring(0, 100) + '...' : full;
      const escaped = escHtml(display);
      const rendered = (isAnon && (k === 'query' || k === 'sanitized_query'))
        ? highlightAnon(escaped) : escaped;
      html += `<td title="${escHtml(full)}">${rendered}</td>`;
    }
    html += '</tr>';
  }
  html += '</tbody></table></div>';
  el.innerHTML = html;
}

// === Log Tabs ===

function switchLogTab(tab) {
  activeLogTab = tab;
  $$('.tab').forEach(t => t.classList.toggle('active', t.textContent.toLowerCase() === tab));
  loadLogTab();
}

async function loadLogTab() {
  if (!currentView) return;
  const el = $('#log-content');
  if (!el) return;

  try {
    if (activeLogTab === 'pipeline') {
      const data = await fetchJSON(`/api/databases/${currentView.id}/pipeline`);
      const events = data.events || [];
      if (events.length === 0) {
        el.innerHTML = '<div class="empty-row">No pipeline events yet</div>';
        return;
      }
      let html = '<div class="log-scroll">';
      for (const ev of events.slice(0, 40)) {
        const collectors = ev.collectors_json || [];
        const collStr = collectors.map(c =>
          `${c.name}: ${fmtNum(c.rows)} ${fmtMs(c.duration_ms)}${c.error ? ' ERR' : ''}`
        ).join(' | ');
        html += '<div class="log-entry">';
        html += `<span class="log-time">${fmtTime(ev.created_at)}</span>`;
        html += `<span class="log-type ${ev.tick_type}">${ev.tick_type}</span>`;
        html += `<span class="log-detail">${escHtml(collStr)}</span>`;
        html += '</div>';
      }
      html += '</div>';
      el.innerHTML = html;
    } else {
      const data = await fetchJSON(`/api/databases/${currentView.id}/shipping`);
      const entries = data.entries || [];
      if (entries.length === 0) {
        el.innerHTML = '<div class="empty-row">No shipping events yet</div>';
        return;
      }
      let html = '<div class="log-scroll">';
      for (const entry of entries.slice(0, 40)) {
        html += '<div class="log-entry">';
        html += `<span class="log-time">${fmtTime(entry.created_at)}</span>`;
        html += `<span class="ship-status ${entry.status}">${entry.status}</span>`;
        html += `<span class="log-detail">${fmtBytes(entry.bytes)}${entry.error ? ' &middot; ' + escHtml(entry.error) : ''}</span>`;
        html += '</div>';
      }
      html += '</div>';
      el.innerHTML = html;
    }
  } catch (e) {
    el.innerHTML = `<div class="empty-row" style="color:var(--red)">Error: ${escHtml(e.message)}</div>`;
  }
}

// === Anonymization helpers ===

const ANON_TOKENS = [
  { re: /&lt;email&gt;/g, label: 'email', cls: 'anon-email' },
  { re: /&lt;uuid&gt;/g, label: 'uuid', cls: 'anon-uuid' },
  { re: /&lt;ip&gt;/g, label: 'ip', cls: 'anon-ip' },
  { re: /&lt;token&gt;/g, label: 'token', cls: 'anon-token' },
  { re: /&lt;card&gt;/g, label: 'card', cls: 'anon-card' },
];
const ANON_RAW_RE = /<(email|uuid|ip|token|card)>/g;

function countAnonTokens(rows) {
  let total = 0;
  const counts = {};
  for (const row of rows) {
    for (const val of Object.values(row)) {
      if (typeof val !== 'string') continue;
      let m;
      ANON_RAW_RE.lastIndex = 0;
      while ((m = ANON_RAW_RE.exec(val)) !== null) {
        total++;
        counts[m[1]] = (counts[m[1]] || 0) + 1;
      }
    }
  }
  return { total, counts };
}

function highlightAnon(escaped) {
  for (const t of ANON_TOKENS) {
    escaped = escaped.replace(t.re, `<span class="anon-tag ${t.cls}">&lt;${t.label}&gt;</span>`);
  }
  return escaped;
}

// === Drawer ===

function openDrawer(id) {
  editingId = id || null;
  $('#drawer').classList.add('open');
  $('#drawer-overlay').classList.add('open');

  if (id) {
    const db = databases.find(d => d.id === id);
    if (db) {
      $('#drawer-title').textContent = 'Edit Database';
      $('#edit-id').value = id;
      $('#db-name').value = db.name;
      const dbType = db.db_type || 'postgres';
      $('#db-type').value = dbType;
      $('#db-env').value = db.environment || 'production';
      $('#db-url').value = db.url;
      $('#db-url').placeholder = URL_PLACEHOLDERS[dbType] || URL_PLACEHOLDERS.postgres;
      $('#db-fast').value = db.fast_interval;
      $('#db-slow').value = db.slow_interval;
      renderCollectorCheckboxes(dbType, db.collectors);
      $('#anon-enabled').checked = db.anonymize !== false;
      updateAnonHint();
      $('#btn-delete').style.display = 'inline-flex';
    }
  } else {
    resetDrawer();
  }
}

function closeDrawer() {
  $('#drawer').classList.remove('open');
  $('#drawer-overlay').classList.remove('open');
  resetDrawer();
}

function resetDrawer() {
  $('#drawer-title').textContent = 'Add Database';
  $('#edit-id').value = '';
  $('#db-name').value = '';
  $('#db-type').value = 'postgres';
  $('#db-env').value = 'production';
  $('#db-url').value = '';
  $('#db-url').placeholder = URL_PLACEHOLDERS.postgres;
  $('#db-fast').value = '30';
  $('#db-slow').value = '300';
  renderCollectorCheckboxes('postgres');
  $('#anon-enabled').checked = true;
  updateAnonHint();
  $('#test-result').className = 'test-result';
  $('#test-result').textContent = '';
  $('#btn-delete').style.display = 'none';
  editingId = null;
}

function onDbTypeChange() {
  const dbType = $('#db-type').value;
  $('#db-url').placeholder = URL_PLACEHOLDERS[dbType] || URL_PLACEHOLDERS.postgres;
  renderCollectorCheckboxes(dbType);
}

function renderCollectorCheckboxes(dbType, selectedCollectors) {
  const group = $('#collectors-group');
  const collectors = COLLECTORS_BY_TYPE[dbType] || COLLECTORS_BY_TYPE.postgres;
  let html = '';
  for (const c of collectors) {
    const checked = selectedCollectors
      ? selectedCollectors.includes(c.name)
      : c.checked;
    html += `<label class="checkbox"><input type="checkbox" value="${c.name}" ${checked ? 'checked' : ''}> ${c.name}</label>`;
  }
  group.innerHTML = html;
}

function onEnvChange() {
  const env = $('#db-env').value;
  if (!$('#edit-id').value) {
    const shouldAnon = env === 'production' || env === 'staging';
    $('#anon-enabled').checked = shouldAnon;
  }
  updateAnonHint();
}

function updateAnonHint() {
  const env = $('#db-env').value;
  const hint = $('#anon-hint');
  if (env === 'development' || env === 'local') {
    hint.textContent = '(optional for ' + env + ')';
  } else {
    hint.textContent = '(recommended for ' + env + ')';
  }
}

function editDatabase(id) { openDrawer(id); }

// === CRUD Actions ===

async function testConnection() {
  const url = $('#db-url').value;
  const dbType = $('#db-type').value;
  const el = $('#test-result');
  el.className = 'test-result loading';
  el.textContent = 'Testing...';
  try {
    const res = await fetchJSON('/api/test-connection', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url, db_type: dbType }),
    });
    if (res.ok) {
      el.className = 'test-result success';
      el.textContent = res.version || 'Connected';
    } else {
      el.className = 'test-result error';
      el.textContent = res.error || 'Failed';
    }
  } catch (e) {
    el.className = 'test-result error';
    el.textContent = e.message;
  }
}

async function saveDatabase(event) {
  event.preventDefault();
  const collectors = $$('#collectors-group input:checked').map(cb => cb.value);
  const body = {
    name: $('#db-name').value,
    db_type: $('#db-type').value,
    environment: $('#db-env').value,
    url: $('#db-url').value,
    fast_interval: parseInt($('#db-fast').value) || 30,
    slow_interval: parseInt($('#db-slow').value) || 300,
    collectors,
    anonymize: $('#anon-enabled').checked,
  };
  const id = $('#edit-id').value;
  const method = id ? 'PUT' : 'POST';
  const path = id ? `/api/databases/${id}` : '/api/databases';
  try {
    const res = await fetchJSON(path, {
      method,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (res.ok) {
      closeDrawer();
      await loadDatabases();
    } else {
      alert(res.error || 'Failed to save');
    }
  } catch (e) {
    alert('Error: ' + e.message);
  }
  return false;
}

async function deleteDatabase() {
  const id = editingId;
  if (!id) return;
  if (!confirm('Remove this database? Collection will stop and data will be deleted.')) return;
  try {
    const res = await fetchJSON(`/api/databases/${id}`, { method: 'DELETE' });
    if (res.ok) {
      closeDrawer();
      navigate('#');
      await loadDatabases();
    } else {
      alert(res.error || 'Failed to delete');
    }
  } catch (e) {
    alert('Error: ' + e.message);
  }
}

// === Shipper Modal ===

function openShipperModal(dbId, shipperType) {
  const modal = $('#shipper-modal');
  modal.style.display = 'block';
  $('#sm-overlay').style.display = 'block';
  modal.dataset.dbId = dbId;
  modal.dataset.shipperType = shipperType;
  $('#sm-type-label').innerHTML = '<span class="modal-icon">' + shipperIcon(shipperType) + '</span> ' + shipperType.charAt(0).toUpperCase() + shipperType.slice(1);
  $('#sm-name').value = shipperType === 'datapace' ? 'Datapace Cloud' : '';
  $('#sm-endpoint').value = shipperType === 'datapace' ? 'https://api.datapace.ai/v1/ingest' : '';
  $('#sm-token').value = '';
  $('#sm-name').focus();
}

function closeShipperModal() {
  $('#shipper-modal').style.display = 'none';
  $('#sm-overlay').style.display = 'none';
}

async function saveShipper() {
  const modal = $('#shipper-modal');
  const dbId = modal.dataset.dbId;
  const shipperType = modal.dataset.shipperType;
  const name = $('#sm-name').value.trim();
  const endpoint = $('#sm-endpoint').value.trim();
  const token = $('#sm-token').value.trim() || null;
  if (!name || !endpoint) { alert('Name and endpoint are required'); return; }
  try {
    const res = await fetchJSON(`/api/databases/${dbId}/shippers`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, shipper_type: shipperType, endpoint, token }),
    });
    if (res.ok) {
      closeShipperModal();
      await loadDatabases();
    } else {
      alert(res.error || 'Failed to add shipper');
    }
  } catch (e) {
    alert('Error: ' + e.message);
  }
}

async function removeShipper(dbId, shipperId) {
  if (!confirm('Remove this shipper destination?')) return;
  try {
    const res = await fetchJSON(`/api/databases/${dbId}/shippers/${shipperId}`, { method: 'DELETE' });
    if (res.ok) {
      await loadDatabases();
    } else {
      alert(res.error || 'Failed to remove shipper');
    }
  } catch (e) {
    alert('Error: ' + e.message);
  }
}

// === Init ===

document.addEventListener('DOMContentLoaded', async () => {
  await loadDatabases();
  route();
  startRefresh();
});

window.addEventListener('hashchange', route);
