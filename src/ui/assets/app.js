// Datapace Agent — Multi-DB Control Plane UI
'use strict';

const API = '';
let databases = [];
let editingId = null;
let refreshTimer = null;
let currentView = null;     // null = list, { id: "..." } = pipeline page
let selectedNode = null;    // which pipeline node is selected on pipeline page

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
  if (!b) return '0B';
  if (b >= 1e6) return (b / 1e6).toFixed(1) + 'MB';
  if (b >= 1e3) return (b / 1e3).toFixed(1) + 'KB';
  return b + 'B';
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

// === Database Type Icons ===

const DB_ICONS = {
  postgres: `<svg viewBox="0 0 24 24" fill="none"><path d="M12 2C6.48 2 2 4.69 2 8v8c0 3.31 4.48 6 10 6s10-2.69 10-6V8c0-3.31-4.48-6-10-6z" fill="none" stroke="currentColor" stroke-width="1.5"/><ellipse cx="12" cy="8" rx="10" ry="4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2 12c0 2.21 4.48 4 10 4s10-1.79 10-4" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>`,
  mysql: `<svg viewBox="0 0 24 24" fill="none"><path d="M12 2C6.48 2 2 4.69 2 8v8c0 3.31 4.48 6 10 6s10-2.69 10-6V8c0-3.31-4.48-6-10-6z" fill="none" stroke="currentColor" stroke-width="1.5"/><ellipse cx="12" cy="8" rx="10" ry="4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2 12c0 2.21 4.48 4 10 4s10-1.79 10-4" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>`,
  mongodb: `<svg viewBox="0 0 24 24" fill="none"><path d="M12 2C6.48 2 2 4.69 2 8v8c0 3.31 4.48 6 10 6s10-2.69 10-6V8c0-3.31-4.48-6-10-6z" fill="none" stroke="currentColor" stroke-width="1.5"/><ellipse cx="12" cy="8" rx="10" ry="4" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M2 12c0 2.21 4.48 4 10 4s10-1.79 10-4" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>`,
};

function dbIcon(dbType) { return DB_ICONS[dbType] || DB_ICONS.postgres; }

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

// === Monochrome SVG Icons ===

const ICON = {
  collect: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="12" width="3" height="6" rx="0.5"/><rect x="8.5" y="7" width="3" height="11" rx="0.5"/><rect x="15" y="2" width="3" height="16" rx="0.5"/></svg>`,
  anonymize: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10 2L3 5.5v4.5c0 4.2 3 7.7 7 8.5 4-.8 7-4.3 7-8.5V5.5L10 2z"/><path d="M7 10l2 2 4-4"/></svg>`,
  store: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5"><ellipse cx="10" cy="5" rx="7" ry="3"/><path d="M3 5v10c0 1.66 3.13 3 7 3s7-1.34 7-3V5"/><path d="M3 10c0 1.66 3.13 3 7 3s7-1.34 7-3"/></svg>`,
  webhook: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M18 3l-8 8"/><path d="M18 3l-5 15-3-7-7-3 15-5z"/></svg>`,
  custom: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 12l-2 2 2 2"/><path d="M12 8l2-2-2-2"/><path d="M7 3h6a4 4 0 014 4v6a4 4 0 01-4 4H7a4 4 0 01-4-4V7a4 4 0 014-4z"/></svg>`,
  datapace: `<img src="/logo.svg" style="width:100%;height:100%;object-fit:contain">`,
  arrow: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg>`,
  back: `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2L4 7l5 5"/></svg>`,
  plus: `<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M7 2v10M2 7h10"/></svg>`,
  empty: `<svg width="48" height="48" viewBox="0 0 48 48" fill="none"><circle cx="24" cy="24" r="20" stroke="rgba(255,255,255,0.12)" stroke-width="1.5" stroke-dasharray="4 4"/><path d="M24 16v16M16 24h16" stroke="rgba(255,255,255,0.2)" stroke-width="1.5" stroke-linecap="round"/></svg>`,
};

function nodeIcon(type) { return ICON[type] || ICON.custom; }
function shipperIcon(type_) { return ICON[type_] || ICON.custom; }

// === Routing ===

function navigate(hash) {
  if (location.hash !== hash) location.hash = hash;
  else route(); // force re-route if same hash
}

function route() {
  const hash = location.hash;
  const match = hash.match(/^#db\/(.+)$/);
  if (match) {
    currentView = { id: match[1] };
    selectedNode = null;
    renderPipelinePage();
  } else {
    currentView = null;
    selectedNode = null;
    renderListPage();
  }
}

// === Data Loading ===

async function loadDatabases() {
  try {
    databases = await fetchJSON('/api/databases');
    if (currentView) {
      updatePipelinePage();
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
//  LIST PAGE — database cards
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
        <p>Add a PostgreSQL database to start collecting metrics.</p>
        <button class="btn-primary" onclick="openDrawer()">${ICON.plus} Add Database</button>
      </div>`;
    return;
  }

  let html = '';
  for (const db of databases) {
    const rs = db.runtime_status;
    const status = rs?.status || db.status || 'stopped';
    const stats = rs?.collector_stats || [];
    const dbType = db.db_type || 'postgres';
    const env = db.environment || 'production';
    const envLabel = ENV_LABELS[env] || env.toUpperCase();
    const shippers = db.shippers || [];
    const totalRows = stats.reduce((sum, s) => sum + (s.rows || 0), 0);

    html += `<div class="db-card" onclick="navigate('#db/${db.id}')">`;
    html += '<div class="db-card-header">';
    html += '<div class="db-card-left">';
    html += `<div class="db-type-icon" title="${escHtml(dbType)}">${dbIcon(dbType)}</div>`;
    html += `<div class="status-dot ${status}"></div>`;
    html += '<div class="db-card-info-row">';
    html += '<div class="db-info">';
    html += `<span class="db-name">${escHtml(db.name)}</span>`;
    html += `<span class="db-url">${escHtml(db.masked_url)}</span>`;
    html += '</div>';
    html += `<span class="env-badge ${env}">${envLabel}</span>`;
    if (db.anonymize) {
      html += `<span class="anon-badge on" title="Query anonymization enabled">anon</span>`;
    } else {
      html += `<span class="anon-badge off" title="Query anonymization disabled">raw</span>`;
    }
    html += '</div></div>';
    html += `<div style="display:flex;align-items:center;gap:0.35rem">`;
    html += `<button class="btn-edit" onclick="event.stopPropagation(); editDatabase('${db.id}')">edit</button>`;
    html += `<span class="db-card-arrow">${ICON.arrow}</span>`;
    html += `</div>`;
    html += '</div>';

    // Summary row
    html += '<div class="db-card-footer">';
    html += `<span class="status-dot-inline ${status}"></span>`;
    html += `<span>${escHtml(status)}</span>`;
    html += `<span>${stats.length} collectors</span>`;
    html += `<span>${fmtNum(totalRows)} rows</span>`;
    html += `<span>${shippers.length} shipper${shippers.length !== 1 ? 's' : ''}</span>`;
    html += `<span>Last: ${timeAgo(rs?.last_tick)}</span>`;
    html += '</div>';
    html += '</div>';
  }

  main.innerHTML = html;
}

// =====================================
//  PIPELINE PAGE — full page per database
// =====================================

function renderPipelinePage() {
  const db = databases.find(d => d.id === currentView.id);
  const main = $('#main-content');
  const headerBtn = $('#header-action');

  headerBtn.innerHTML = `<button class="btn-secondary" onclick="navigate('#')">${ICON.back} All Databases</button>`;

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

  let html = '';

  // Page header
  html += '<div class="pipe-page-header">';
  html += `<div class="db-type-icon">${dbIcon(dbType)}</div>`;
  html += `<div class="status-dot ${status}"></div>`;
  html += `<h1 class="pipe-page-title">${escHtml(db.name)}</h1>`;
  html += `<span class="env-badge ${env}">${envLabel}</span>`;
  if (db.anonymize) html += `<span class="anon-badge on">anon</span>`;
  else html += `<span class="anon-badge off">raw</span>`;
  html += `<button class="btn-edit" onclick="editDatabase('${db.id}')" style="margin-left:auto">edit</button>`;
  html += '</div>';
  html += `<div class="pipe-page-url">${escHtml(db.masked_url)}</div>`;

  // Visual pipeline
  html += buildPipelineNodes(db, rs, stats, shippers, shipperStatuses);

  // Detail panel — shown when a node is clicked
  html += '<div id="pipe-detail" class="pipe-detail"></div>';

  main.innerHTML = html;

  // Auto-select first node if none
  if (!selectedNode) {
    selectNode(stats.length > 0 ? 'collect' : 'store');
  } else {
    selectNode(selectedNode);
  }
}

function updatePipelinePage() {
  if (!currentView) return;
  const db = databases.find(d => d.id === currentView.id);
  if (!db) return;

  const rs = db.runtime_status;
  const stats = rs?.collector_stats || [];
  const shippers = db.shippers || [];
  const shipperStatuses = rs?.shipper_statuses || [];

  // Re-render pipeline + palette as one unit
  const wrapEl = $('.pipeline-wrap');
  if (wrapEl) {
    wrapEl.outerHTML = buildPipelineNodes(db, rs, stats, shippers, shipperStatuses);
  }

  // Re-highlight selected node
  if (selectedNode) highlightNode(selectedNode);
}

function buildPipelineNodes(db, rs, stats, shippers, shipperStatuses) {
  const hasCollect = stats.length > 0;
  const hasError = stats.some(s => s.error);
  const hasAnon = db.anonymize;
  const totalRows = stats.reduce((sum, s) => sum + (s.rows || 0), 0);
  const totalDur = stats.reduce((sum, s) => sum + (s.duration_ms || 0), 0);

  let html = '<div class="pipeline-wrap">';
  html += '<div class="pipeline-visual">';

  // Collect
  const cStatus = hasCollect ? (hasError ? 'err' : 'ok') : 'off';
  html += `<div class="pipe-node ${cStatus} ${selectedNode === 'collect' ? 'selected' : ''}" data-node="collect" onclick="selectNode('collect')">`;
  html += `<div class="pipe-node-icon">${nodeIcon('collect')}</div>`;
  html += `<div class="pipe-node-text">`;
  html += '<div class="pipe-node-label">Collect</div>';
  html += `<div class="pipe-node-detail">${stats.length} collectors &middot; ${fmtNum(totalRows)} rows &middot; ${fmtMs(totalDur)}</div>`;
  html += '</div></div>';

  html += `<div class="pipe-connector ${hasCollect ? 'flowing' : ''}"></div>`;

  // Anonymize
  if (hasAnon) {
    html += `<div class="pipe-node ${hasCollect ? 'ok' : 'off'} ${selectedNode === 'anonymize' ? 'selected' : ''}" data-node="anonymize" onclick="selectNode('anonymize')">`;
    html += `<div class="pipe-node-icon">${nodeIcon('anonymize')}</div>`;
    html += `<div class="pipe-node-text">`;
    html += '<div class="pipe-node-label">Anonymize</div>';
    html += '<div class="pipe-node-detail">scrub PII</div>';
    html += '</div></div>';
    html += `<div class="pipe-connector ${hasCollect ? 'flowing' : ''}"></div>`;
  }

  // Store
  html += `<div class="pipe-node ${hasCollect ? 'ok' : 'off'} ${selectedNode === 'store' ? 'selected' : ''}" data-node="store" onclick="selectNode('store')">`;
  html += `<div class="pipe-node-icon">${nodeIcon('store')}</div>`;
  html += `<div class="pipe-node-text">`;
  html += '<div class="pipe-node-label">Store</div>';
  html += '<div class="pipe-node-detail">SQLite</div>';
  html += '</div></div>';

  html += `<div class="pipe-connector ${hasCollect ? 'flowing' : ''}"></div>`;

  // Shippers fan-out
  html += '<div class="pipe-shipper-group">';
  for (const shipper of shippers) {
    const ss = shipperStatuses.find(s => s.shipper_id === shipper.id);
    const sStatus = ss ? ss.status : 'off';
    const nodeId = 'shipper:' + shipper.id;
    const ep = shipper.endpoint.length > 25 ? shipper.endpoint.substring(0, 25) + '...' : shipper.endpoint;
    const timeStr = ss?.at ? timeAgo(ss.at) : '';
    html += `<div class="pipe-node shipper ${sStatus} ${shipper.enabled ? '' : 'disabled'} ${selectedNode === nodeId ? 'selected' : ''}" data-node="${escHtml(nodeId)}" onclick="selectNode('${escHtml(nodeId)}')">`;
    html += `<div class="pipe-node-icon">${shipperIcon(shipper.shipper_type)}</div>`;
    html += `<div class="pipe-node-text">`;
    html += `<div class="pipe-node-label">${escHtml(shipper.name)}</div>`;
    html += `<div class="pipe-node-detail">${escHtml(ep)}${timeStr ? ' &middot; ' + timeStr : ''}</div>`;
    html += '</div>';
    html += `<button class="pipe-node-remove" onclick="event.stopPropagation(); removeShipper('${db.id}', '${shipper.id}')" title="Remove"><svg width="8" height="8" viewBox="0 0 8 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M1 1l6 6M7 1L1 7"/></svg></button>`;
    html += '</div>';
  }
  // Drop zone
  html += `<div class="pipe-drop-zone" data-db-id="${db.id}" ondragover="onPipeDragOver(event)" ondragleave="onPipeDragLeave(event)" ondrop="onPipeDrop(event)">`;
  html += `<span>${ICON.plus} add shipper</span>`;
  html += '</div>';
  html += '</div>';

  html += '</div>';

  // Palette — single inline row
  html += '<div class="shipper-palette">';
  html += `<span class="palette-label">Drag to add</span>`;
  html += `<div class="palette-item" draggable="true" ondragstart="onPaletteDragStart(event, 'datapace')"><span class="palette-icon datapace-logo">${ICON.datapace}</span> Datapace</div>`;
  html += `<div class="palette-item" draggable="true" ondragstart="onPaletteDragStart(event, 'webhook')"><span class="palette-icon">${ICON.webhook}</span> Webhook</div>`;
  html += `<div class="palette-item" draggable="true" ondragstart="onPaletteDragStart(event, 'custom')"><span class="palette-icon">${ICON.custom}</span> Custom</div>`;
  html += '</div>';
  html += '</div>'; // close .pipeline-wrap

  return html;
}

// === Node Selection + Detail ===

function selectNode(nodeId) {
  selectedNode = nodeId;
  highlightNode(nodeId);
  loadNodeDetail(nodeId);
}

function highlightNode(nodeId) {
  $$('.pipe-node').forEach(el => {
    el.classList.toggle('selected', el.dataset.node === nodeId);
  });
}

async function loadNodeDetail(nodeId) {
  const el = $('#pipe-detail');
  if (!el || !currentView) return;
  const db = databases.find(d => d.id === currentView.id);
  if (!db) return;
  const rs = db.runtime_status;
  const stats = rs?.collector_stats || [];

  el.innerHTML = '<div style="padding:1rem;font-size:0.8rem;color:var(--text-muted)">Loading...</div>';

  try {
    if (nodeId === 'collect') {
      await renderCollectDetail(el, db, stats);
    } else if (nodeId === 'anonymize') {
      await renderAnonymizeDetail(el, db);
    } else if (nodeId === 'store') {
      await renderStoreDetail(el, db);
    } else if (nodeId.startsWith('shipper:')) {
      const shipperId = nodeId.replace('shipper:', '');
      await renderShipperDetail(el, db, shipperId);
    }
  } catch (e) {
    el.innerHTML = `<div class="err" style="padding:1rem;font-size:0.8rem">Error: ${escHtml(e.message)}</div>`;
  }
}

// -- Collect detail: tabs per collector --

async function renderCollectDetail(el, db, stats) {
  const collectors = db.collectors || [];
  if (collectors.length === 0) {
    el.innerHTML = '<div class="detail-empty">No collectors configured</div>';
    return;
  }

  // Per-collector stat cards + tab selector
  let html = '<div class="detail-header"><h3><span class="detail-icon">' + nodeIcon('collect') + '</span> Collectors</h3></div>';

  // Stat cards
  html += '<div class="collector-cards">';
  for (const name of collectors) {
    const stat = stats.find(s => s.name === name);
    const hasErr = stat?.error;
    html += `<div class="collector-card ${hasErr ? 'err' : (stat ? 'ok' : 'off')}" onclick="loadCollectorData('${escHtml(name)}')">`;
    html += `<div class="cc-name">${escHtml(name)}</div>`;
    if (stat) {
      html += `<div class="cc-stat">${fmtNum(stat.rows)} rows \u00B7 ${fmtMs(stat.duration_ms)}</div>`;
      if (hasErr) html += `<div class="cc-err">${escHtml(stat.error)}</div>`;
    } else {
      html += '<div class="cc-stat">no data</div>';
    }
    html += '</div>';
  }
  html += '</div>';

  html += '<div id="collector-data-panel"></div>';
  el.innerHTML = html;

  // Load first collector
  loadCollectorData(collectors[0]);
}

async function loadCollectorData(name) {
  if (!currentView) return;
  const panel = $('#collector-data-panel');
  if (!panel) return;

  // Highlight selected card
  $$('.collector-card').forEach(c => c.classList.toggle('active', c.querySelector('.cc-name')?.textContent === name));

  panel.innerHTML = '<div style="padding:0.5rem;font-size:0.8rem;color:var(--text-muted)">Loading...</div>';

  const data = await fetchJSON(`/api/databases/${currentView.id}/collectors/${name}`);
  renderCollectorDataPanel(panel, name, data.snapshot);
}

function renderCollectorDataPanel(el, name, snapshot) {
  if (!snapshot || !snapshot.data) {
    el.innerHTML = '<div class="detail-empty">No data yet</div>';
    return;
  }

  const rows = Array.isArray(snapshot.data) ? snapshot.data : [snapshot.data];
  if (rows.length === 0) {
    el.innerHTML = '<div class="detail-empty">Empty</div>';
    return;
  }

  const db = databases.find(d => d.id === currentView.id);
  const isAnon = db?.anonymize;
  const anonInfo = isAnon ? countAnonTokens(rows) : { total: 0, counts: {} };

  const ts = snapshot.collected_at ? timeAgo(snapshot.collected_at) : '';
  let html = '<div class="collector-meta">';
  html += `<span>${escHtml(name)} \u00B7 ${rows.length} rows \u00B7 ${ts}</span>`;
  if (isAnon && anonInfo.total > 0) {
    const breakdown = Object.entries(anonInfo.counts).map(([k, v]) => `${v} ${k}`).join(', ');
    html += `<span class="anon-summary" title="${escHtml(breakdown)}">`;
    html += `<svg class="anon-shield" viewBox="0 0 16 16" width="12" height="12"><path d="M8 1L2 4v4c0 3.5 2.6 6.4 6 7 3.4-.6 6-3.5 6-7V4L8 1z" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.5 8l2 2 3.5-4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
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

// -- Anonymize detail --

async function renderAnonymizeDetail(el, db) {
  let html = '<div class="detail-header"><h3><span class="detail-icon">' + nodeIcon('anonymize') + '</span> Anonymization</h3></div>';
  html += '<div class="detail-info">';
  html += `<p>Status: <strong class="ok">${db.anonymize ? 'Enabled' : 'Disabled'}</strong></p>`;
  html += '<p>Scrubs emails, UUIDs, IPs, tokens, and credit card numbers from collected queries before storage.</p>';
  html += '</div>';

  // Show recent pipeline events for context
  const data = await fetchJSON(`/api/databases/${db.id}/pipeline`);
  const events = data.events || [];
  if (events.length > 0) {
    html += '<div class="detail-subheader">Recent Pipeline Events</div>';
    html += renderPipelineEvents(events);
  }

  el.innerHTML = html;
}

// -- Store detail --

async function renderStoreDetail(el, db) {
  let html = '<div class="detail-header"><h3><span class="detail-icon">' + nodeIcon('store') + '</span> Local Store</h3></div>';
  html += '<div class="detail-info">';
  html += '<p>Engine: <strong>SQLite</strong></p>';
  html += '<p>All collected snapshots are stored locally before shipping.</p>';
  html += '</div>';

  const data = await fetchJSON(`/api/databases/${db.id}/pipeline`);
  const events = data.events || [];
  if (events.length > 0) {
    html += '<div class="detail-subheader">Recent Pipeline Events</div>';
    html += renderPipelineEvents(events);
  }

  el.innerHTML = html;
}

// -- Shipper detail --

async function renderShipperDetail(el, db, shipperId) {
  const shipper = db.shippers.find(s => s.id === shipperId);
  if (!shipper) {
    el.innerHTML = '<div class="detail-empty">Shipper not found</div>';
    return;
  }

  const rs = db.runtime_status;
  const ss = (rs?.shipper_statuses || []).find(s => s.shipper_id === shipperId);

  let html = '<div class="detail-header">';
  html += `<h3><span class="detail-icon">${shipperIcon(shipper.shipper_type)}</span> ${escHtml(shipper.name)}</h3>`;
  html += `<button class="btn-danger btn-sm" onclick="removeShipper('${db.id}', '${shipperId}')">Remove</button>`;
  html += '</div>';

  html += '<div class="detail-info">';
  html += `<p>Type: <strong>${escHtml(shipper.shipper_type)}</strong></p>`;
  html += `<p>Endpoint: <code>${escHtml(shipper.endpoint)}</code></p>`;
  html += `<p>Token: <code>${shipper.token ? '***' : 'none'}</code></p>`;
  html += `<p>Enabled: <strong>${shipper.enabled ? 'Yes' : 'No'}</strong></p>`;
  if (ss) {
    html += `<p>Last status: <strong class="${ss.status === 'ok' ? 'ok' : 'err'}">${ss.status}</strong> \u00B7 ${fmtBytes(ss.bytes)} \u00B7 ${timeAgo(ss.at)}</p>`;
    if (ss.error) html += `<p class="err">${escHtml(ss.error)}</p>`;
  }
  html += '</div>';

  // Shipping log
  const data = await fetchJSON(`/api/databases/${db.id}/shipping`);
  const entries = (data.entries || []).filter(e => e.shipper_id === shipperId || !e.shipper_id);
  if (entries.length > 0) {
    html += '<div class="detail-subheader">Shipping Log</div>';
    html += '<div class="pipeline-log">';
    for (const entry of entries) {
      html += `<div class="ship-entry">`;
      html += `<span style="color:var(--text-muted);width:60px">${fmtTime(entry.created_at)}</span>`;
      html += `<span class="se-status ${entry.status}">${entry.status}</span>`;
      html += `<span>${fmtBytes(entry.bytes)}</span>`;
      if (entry.error) html += `<span class="err">${escHtml(entry.error)}</span>`;
      html += '</div>';
    }
    html += '</div>';
  }

  el.innerHTML = html;
}

// -- Shared renderers --

function renderPipelineEvents(events) {
  let html = '<div class="pipeline-log">';
  for (const ev of events.slice(0, 30)) {
    const collectors = ev.collectors_json || [];
    const collStr = collectors.map(c =>
      `${c.name}: ${fmtNum(c.rows)} ${fmtMs(c.duration_ms)}${c.error ? ' ERR' : ''}`
    ).join(' | ');
    html += `<div class="pipeline-entry">`;
    html += `<span class="pe-time">${fmtTime(ev.created_at)}</span>`;
    html += `<span class="pe-type pe-${ev.tick_type}">${ev.tick_type}</span>`;
    html += `<span class="pe-collectors">${escHtml(collStr)}</span>`;
    html += '</div>';
  }
  html += '</div>';
  return html;
}

// Anonymization helpers
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
  // Update URL placeholder
  $('#db-url').placeholder = URL_PLACEHOLDERS[dbType] || URL_PLACEHOLDERS.postgres;
  // Swap collector checkboxes
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

// === Drag and Drop ===

function onPaletteDragStart(e, shipperType) {
  e.dataTransfer.setData('text/plain', shipperType);
  e.dataTransfer.effectAllowed = 'copy';
}

function onPipeDragOver(e) {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
  e.currentTarget.classList.add('drag-over');
}

function onPipeDragLeave(e) {
  e.currentTarget.classList.remove('drag-over');
}

function onPipeDrop(e) {
  e.preventDefault();
  e.currentTarget.classList.remove('drag-over');
  const shipperType = e.dataTransfer.getData('text/plain');
  const dbId = e.currentTarget.dataset.dbId;
  if (!shipperType || !dbId) return;
  openShipperModal(dbId, shipperType);
}

// === Shipper Modal ===

function openShipperModal(dbId, shipperType) {
  const modal = $('#shipper-modal');
  modal.style.display = 'block';
  $('#sm-overlay').style.display = 'block';
  modal.dataset.dbId = dbId;
  modal.dataset.shipperType = shipperType;
  $('#sm-type-label').innerHTML = '<span class="sm-icon">' + shipperIcon(shipperType) + '</span> ' + shipperType.charAt(0).toUpperCase() + shipperType.slice(1);
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
      selectedNode = null;
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
