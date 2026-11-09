'use strict';
let state = null;
let preferencesDirty = false;
let shieldOpen = false;
let toastTimer;
const byId = id => document.getElementById(id);
const send = (action, values = {}) => window.ipc.postMessage(JSON.stringify({ action, ...values }));
const make = (tag, text, className) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
};
function createSvgIcon(name) {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  if (name === 'close') {
    svg.setAttribute('width', '10');
    svg.setAttribute('height', '10');
    svg.setAttribute('viewBox', '0 0 16 16');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '2');
    svg.setAttribute('stroke-linecap', 'round');
    const l1 = document.createElementNS(ns, 'line');
    l1.setAttribute('x1', '3'); l1.setAttribute('y1', '3'); l1.setAttribute('x2', '13'); l1.setAttribute('y2', '13');
    const l2 = document.createElementNS(ns, 'line');
    l2.setAttribute('x1', '13'); l2.setAttribute('y1', '3'); l2.setAttribute('x2', '3'); l2.setAttribute('y2', '13');
    svg.append(l1, l2);
  } else if (name === 'spinner') {
    svg.setAttribute('class', 'tab-spinner');
    svg.setAttribute('width', '11');
    svg.setAttribute('height', '11');
    svg.setAttribute('viewBox', '0 0 16 16');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '2.2');
    svg.setAttribute('stroke-linecap', 'round');
    const p = document.createElementNS(ns, 'path');
    p.setAttribute('d', 'M8 2a6 6 0 1 0 6 6');
    svg.append(p);
  } else if (name === 'audio') {
    svg.setAttribute('width', '12');
    svg.setAttribute('height', '12');
    svg.setAttribute('viewBox', '0 0 16 16');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '1.6');
    svg.setAttribute('stroke-linecap', 'round');
    svg.setAttribute('stroke-linejoin', 'round');
    const p1 = document.createElementNS(ns, 'path');
    p1.setAttribute('d', 'M2.5 5.5h2.5l3.5-3v11l-3.5-3h-2.5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1z');
    p1.setAttribute('fill', 'currentColor');
    const p2 = document.createElementNS(ns, 'path');
    p2.setAttribute('d', 'M12 5a4.5 4.5 0 0 1 0 6');
    const p3 = document.createElementNS(ns, 'path');
    p3.setAttribute('d', 'M14 3a7.5 7.5 0 0 1 0 10');
    svg.append(p1, p2, p3);
  } else if (name === 'audio_muted') {
    svg.setAttribute('width', '12');
    svg.setAttribute('height', '12');
    svg.setAttribute('viewBox', '0 0 16 16');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '1.6');
    svg.setAttribute('stroke-linecap', 'round');
    svg.setAttribute('stroke-linejoin', 'round');
    const p1 = document.createElementNS(ns, 'path');
    p1.setAttribute('d', 'M2.5 5.5h2.5l3.5-3v11l-3.5-3h-2.5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1z');
    p1.setAttribute('fill', 'currentColor');
    const l = document.createElementNS(ns, 'line');
    l.setAttribute('x1', '1'); l.setAttribute('y1', '1'); l.setAttribute('x2', '15'); l.setAttribute('y2', '15');
    l.setAttribute('stroke', 'currentColor');
    l.setAttribute('stroke-width', '2');
    svg.append(p1, l);
  }
  return svg;
}
function toast(message) {
  clearTimeout(toastTimer);
  byId('toast').textContent = message;
  byId('toast').style.display = 'block';
  toastTimer = setTimeout(() => { byId('toast').style.display = 'none'; }, 4500);
}
function updateHeaderHeight() {
  const active = state?.tabs.find(t => t.id === state.active);
  const isInternal = active && ['papyrus://settings', 'papyrus://history', 'papyrus://bookmarks', 'papyrus://downloads', 'papyrus://passwords', 'papyrus://extensions'].includes(active.url);
  if (isInternal || state?.clearing) return;

  const showBookmarks = !!state?.preferences?.show_bookmarks;
  let neededHeight = 0;

  if (shieldOpen) {
    neededHeight = Math.max(neededHeight, showBookmarks ? 400 : 370);
  }
  if (byId('clear-confirm') && !byId('clear-confirm').hidden) {
    neededHeight = Math.max(neededHeight, showBookmarks ? 320 : 290);
  }
  if (byId('update-toast') && !byId('update-toast').hidden) {
    neededHeight = Math.max(neededHeight, showBookmarks ? 250 : 220);
  }

  send('header_height', { height: neededHeight });
}
function closeShield() { shieldOpen = false; byId('shield-panel').hidden = true; updateHeaderHeight(); }
function setPreferenceFields(p) {
  for (const key of ['engine', 'startup', 'zoom', 'theme', 'download_dir']) byId(key).value = p[key];
  for (const key of ['show_bookmarks', 'adblock', 'block_popups']) byId(key).checked = p[key];
  byId('allowed_hosts').value = p.allowed_hosts.join('\n');
  byId('blocked_hosts').value = p.blocked_hosts.join('\n');
}
function getFavicon(url) {
  if (!url || url === 'papyrus://newtab' || url === 'about:blank') {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Ccircle cx='8' cy='8' r='6'/%3E%3Cline x1='8' y1='5' x2='8' y2='11'/%3E%3Cline x1='5' y1='8' x2='11' y2='8'/%3E%3C/svg%3E";
  }
  if (url.startsWith('papyrus://history')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Ccircle cx='8' cy='8' r='6'/%3E%3Cpolyline points='8 4 8 8 11 10'/%3E%3C/svg%3E";
  }
  if (url.startsWith('papyrus://bookmarks')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Cpath d='M4 2h8a1 1 0 0 1 1 1v11l-5-3.2L3 14V3a1 1 0 0 1 1-1z'/%3E%3C/svg%3E";
  }
  if (url.startsWith('papyrus://downloads')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Cpath d='M8 2v8m-3-3l3 3 3-3M2 13h12'/%3E%3C/svg%3E";
  }
  if (url.startsWith('papyrus://passwords')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Crect x='3' y='7' width='10' height='7' rx='1'/%3E%3Cpath d='M5 7V4a3 3 0 0 1 6 0v3'/%3E%3C/svg%3E";
  }
  if (url.startsWith('papyrus://extensions')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Cpath d='M3 6h3V3a2 2 0 1 1 4 0v3h3v3h-3a2 2 0 1 0 0 4h3v3H3v-3a2 2 0 1 1 0-4H3V6z'/%3E%3C/svg%3E";
  }
  if (url.startsWith('papyrus://settings')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Ccircle cx='8' cy='8' r='2.5'/%3E%3Cpath d='M8 1v2m0 10v2M1 8h2m10 0h2'/%3E%3C/svg%3E";
  }
  if (url.startsWith('file://')) {
    return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Cpath d='M3 2h7l4 4v8H3V2z'/%3E%3Cpolyline points='10 2 10 6 14 6'/%3E%3C/svg%3E";
  }
  try {
    const host = new URL(url).hostname;
    if (host) {
      return `https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=32`;
    }
  } catch (e) {}
  return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Ccircle cx='8' cy='8' r='6'/%3E%3Cline x1='2' y1='8' x2='14' y2='8'/%3E%3Cpath d='M8 2a9 9 0 0 0 0 12'/%3E%3Cpath d='M8 2a9 9 0 0 1 0 12'/%3E%3C/svg%3E";
}

function renderTabs() {
  const container = byId('tabs-container');
  container.replaceChildren();
  for (const tab of state.tabs) {
    const isPinned = !!tab.pinned;
    const item = make('div', undefined, 'tab' + (tab.id === state.active ? ' active' : '') + (isPinned ? ' pinned' : ''));
    item.dataset.id = tab.id;
    item.dataset.pinned = String(isPinned);
    item.setAttribute('role', 'presentation');
    item.draggable = true;
    item.addEventListener('dragstart', e => {
      e.dataTransfer.setData('text/plain', String(tab.id));
      e.dataTransfer.effectAllowed = 'move';
      item.classList.add('dragging');
      window.draggedTabPinned = isPinned;
    });
    item.addEventListener('dragend', () => {
      item.classList.remove('dragging');
      window.draggedTabPinned = undefined;
      document.querySelectorAll('.tab.drag-over').forEach(el => el.classList.remove('drag-over'));
    });
    item.addEventListener('dragover', e => {
      if (window.draggedTabPinned !== undefined && window.draggedTabPinned !== isPinned) {
        return;
      }
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      item.classList.add('drag-over');
    });
    item.addEventListener('dragleave', () => {
      item.classList.remove('drag-over');
    });
    item.addEventListener('drop', e => {
      e.preventDefault();
      item.classList.remove('drag-over');
      const fromId = Number(e.dataTransfer.getData('text/plain'));
      if (fromId && fromId !== tab.id) {
        const fromTab = state.tabs.find(t => t.id === fromId);
        if (fromTab && !!fromTab.pinned === isPinned) {
          send('reorder_tab', { from: fromId, to: tab.id });
        }
      }
    });

    const select = make('button', undefined, 'tab-select');
    select.setAttribute('role', 'tab'); select.setAttribute('aria-selected', String(tab.id === state.active));
    select.title = tab.title || tab.url;
    if (tab.loading) {
      select.append(createSvgIcon('spinner'));
    } else {
      const fav = make('img', undefined, 'tab-fav');
      fav.src = getFavicon(tab.url);
      fav.alt = '';
      fav.onerror = () => {
        fav.src = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Ccircle cx='8' cy='8' r='6'/%3E%3Cline x1='2' y1='8' x2='14' y2='8'/%3E%3Cpath d='M8 2a9 9 0 0 0 0 12'/%3E%3Cpath d='M8 2a9 9 0 0 1 0 12'/%3E%3C/svg%3E";
      };
      select.append(fav);
    }
    if (!isPinned) {
      const label = make('span', tab.title || 'Tab mới', 'tab-title');
      label.style.overflow = 'hidden';
      label.style.textOverflow = 'ellipsis';
      label.style.whiteSpace = 'nowrap';
      select.append(label);
    }
    select.addEventListener('click', () => send('switch_tab', { id: tab.id }));
    item.append(select);

    // Audio / Speaker Indicator
    if (tab.is_playing_audio || tab.muted) {
      const audioBtn = make('button', undefined, 'tab-audio' + (tab.muted ? ' muted' : ' playing'));
      audioBtn.setAttribute('type', 'button');
      audioBtn.setAttribute('aria-label', tab.muted ? 'Bật tiếng tab' : 'Tắt tiếng tab');
      audioBtn.title = tab.muted ? 'Bật tiếng tab (Ctrl+M)' : 'Tắt tiếng tab (Ctrl+M)';
      audioBtn.append(createSvgIcon(tab.muted ? 'audio_muted' : 'audio'));
      audioBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        send('toggle_mute_tab', { id: tab.id });
      });
      item.append(audioBtn);
    }

    if (!isPinned) {
      const close = make('button', undefined, 'tab-close'); close.setAttribute('aria-label', 'Đóng ' + (tab.title || 'tab'));
      close.append(createSvgIcon('close'));
      close.addEventListener('click', (e) => {
        e.stopPropagation();
        send('close_tab', { id: tab.id });
      });
      item.append(close);
    }
    container.append(item);
  }
}
function renderBookmarks() {
  const container = byId('pills-container'); container.replaceChildren();
  for (const [index, bookmark] of state.bookmarks.entries()) {
    const pill = make('span', undefined, 'pill');
    const link = make('button', bookmark.title); link.title = bookmark.url;
    link.addEventListener('click', () => send('navigate', { input: bookmark.url }));
    const del = make('button', undefined, 'pill-del'); del.setAttribute('aria-label', 'Bỏ ghim ' + bookmark.title);
    del.append(createSvgIcon('close'));
    del.addEventListener('click', () => send('delete_bookmark', { index }));
    pill.append(link, del); container.append(pill);
  }
}

function renderBookmarksManager() {
  const container = byId('bookmarks-list');
  if (!container || !state) return;
  const filter = (byId('bookmarks-search')?.value || '').toLowerCase().trim();
  const entries = (state.bookmarks || [])
    .map((b, originalIndex) => ({ ...b, originalIndex }))
    .filter(b => !filter || (b.title && b.title.toLowerCase().includes(filter)) || (b.url && b.url.toLowerCase().includes(filter)));

  if (entries.length === 0) {
    container.innerHTML = `<div class="history-empty">${state.bookmarks?.length ? 'Không tìm thấy dấu trang phù hợp.' : 'Chưa có dấu trang nào được lưu. Bấm "+ Thêm dấu trang" ở trên hoặc nhấn Ctrl+D khi đang duyệt web.'}</div>`;
    return;
  }

  container.replaceChildren();
  for (const item of entries) {
    const row = make('div', undefined, 'bookmark-item');
    const left = make('div', undefined, 'bookmark-left');

    const fav = make('img', undefined, 'bookmark-fav');
    fav.src = getFavicon(item.url);
    fav.alt = '';
    fav.onerror = () => {
      fav.src = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 16 16' fill='none' stroke='%238C6941' stroke-width='1.5'%3E%3Ccircle cx='8' cy='8' r='6'/%3E%3Cline x1='2' y1='8' x2='14' y2='8'/%3E%3C/svg%3E";
    };

    const title = make('a', item.title || item.url, 'bookmark-title');
    title.title = item.url;
    title.href = '#';
    title.addEventListener('click', e => {
      e.preventDefault();
      send('navigate', { input: item.url });
    });
    title.addEventListener('auxclick', e => {
      if (e.button === 1) {
        e.preventDefault();
        send('navigate', { input: item.url });
      }
    });

    const urlSpan = make('span', item.url, 'bookmark-url');
    left.append(fav, title, urlSpan);

    const actions = make('div', undefined, 'bookmark-actions');

    const copyBtn = make('button', 'Sao chép');
    copyBtn.title = 'Sao chép đường dẫn';
    copyBtn.addEventListener('click', () => {
      navigator.clipboard.writeText(item.url).then(() => toast('Đã sao chép liên kết!'));
    });

    const delBtn = make('button', undefined, 'history-del');
    delBtn.title = 'Xóa dấu trang';
    delBtn.append(createSvgIcon('close'));
    delBtn.addEventListener('click', () => {
      if (window.confirm(`Xóa dấu trang "${item.title}"?`)) {
        send('delete_bookmark', { index: item.originalIndex });
      }
    });

    actions.append(copyBtn, delBtn);
    row.append(left, actions);
    container.append(row);
  }
}

function formatTime(unixSeconds) {
  if (!unixSeconds) return '';
  const d = new Date(unixSeconds * 1000);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const timeStr = d.getHours().toString().padStart(2, '0') + ':' + d.getMinutes().toString().padStart(2, '0');
  if (isToday) return timeStr;
  const dateStr = d.getDate().toString().padStart(2, '0') + '/' + (d.getMonth() + 1).toString().padStart(2, '0');
  return dateStr + ' ' + timeStr;
}

function renderHistory() {
  const container = byId('history-list');
  if (!container || !state || !state.history) return;
  const filter = (byId('history-search')?.value || '').toLowerCase().trim();
  container.replaceChildren();

  const entries = state.history.filter(h => {
    if (!filter) return true;
    return (h.title && h.title.toLowerCase().includes(filter)) || (h.url && h.url.toLowerCase().includes(filter));
  });

  if (entries.length === 0) {
    const empty = make('div', filter ? 'Không tìm thấy kết quả phù hợp.' : 'Chưa có lịch sử duyệt web.', 'history-empty');
    container.append(empty);
    return;
  }

  for (const [idx, item] of entries.entries()) {
    const row = make('div', undefined, 'history-item');
    const left = make('div', undefined, 'history-left');
    const time = make('span', formatTime(item.time), 'history-time');
    const title = make('a', item.title || item.url, 'history-title');
    title.title = item.url;
    title.addEventListener('click', e => { e.preventDefault(); send('navigate', { input: item.url }); });
    const url = make('span', item.url, 'history-url');
    left.append(time, title, url);

    const del = make('button', undefined, 'history-del');
    del.title = 'Xóa mục này khỏi lịch sử';
    del.append(createSvgIcon('close'));
    del.addEventListener('click', () => {
      const realIndex = state.history.indexOf(item);
      if (realIndex >= 0) send('delete_history_entry', { index: realIndex });
    });

    row.append(left, del);
    container.append(row);
  }
}

function renderDownloads() {
  const container = byId('downloads-panel-list');
  if (!container || !state) return;
  if (!state.downloads || state.downloads.length === 0) {
    container.innerHTML = '<div class="history-empty">Chưa có tệp nào được tải xuống.</div>';
    return;
  }
  container.replaceChildren();

  // Show newest downloads at the top (reverse chronological order)
  const list = [...state.downloads].reverse();

  for (const item of list) {
    let status = '';
    let path = item;
    const splitIdx = item.indexOf(': ');
    if (splitIdx !== -1) {
      status = item.substring(0, splitIdx).trim();
      path = item.substring(splitIdx + 2).trim();
    }
    const name = path.split(/[/\\]/).filter(Boolean).pop() || path;

    const card = make('div', undefined, 'download-item');

    const info = make('div', undefined, 'download-info');
    const title = make('div', name, 'download-title');
    title.title = path;

    const meta = make('div', undefined, 'download-meta');
    if (status && status !== 'Đã tải') {
      const badge = make('span', `[${status}] `, 'download-status-badge');
      meta.append(badge);
    }
    meta.append(document.createTextNode(path));

    info.append(title, meta);

    const actions = make('div', undefined, 'download-actions');

    const openBtn = make('button', 'Mở tệp', 'primary');
    openBtn.title = 'Mở tệp với ứng dụng mặc định trên Windows';
    openBtn.addEventListener('click', () => {
      send('open_downloaded_file', { path });
    });

    const folderBtn = make('button', 'Hiển thị trong thư mục');
    folderBtn.title = 'Mở thư mục chứa tệp trong Windows Explorer';
    folderBtn.addEventListener('click', () => {
      send('show_download_in_folder', { path });
    });

    actions.append(openBtn, folderBtn);
    card.append(info, actions);
    container.append(card);
  }
}

const STORE_EXTENSIONS = [
  {
    id: 'ublock-origin',
    name: 'uBlock Origin',
    author: 'Raymond Hill (gorhill)',
    cat: 'security',
    catName: 'Bảo mật & QC',
    icon: '🛡️',
    badge: 'Khuyên dùng',
    desc: 'Trình chặn quảng cáo, mã độc và tracker theo dõi toàn diện số 1 thế giới với hiệu năng siêu nhẹ.',
    url: 'https://chromewebstore.google.com/detail/ublock-origin/cjpalhdlnbpafiamejdnhcphjbkeiagm'
  },
  {
    id: 'dark-reader',
    name: 'Dark Reader',
    author: 'Alexander Shutau',
    cat: 'appearance',
    catName: 'Giao diện',
    icon: '🌙',
    badge: 'Mã nguồn mở',
    desc: 'Chế độ tối (Dark Mode) thông minh cho mọi website, bảo vệ mắt khi duyệt web ban đêm.',
    url: 'https://chromewebstore.google.com/detail/dark-reader/eimadpbcbfnmbkopoojfekhnkhdbieeh'
  },
  {
    id: 'bitwarden',
    name: 'Bitwarden Password Manager',
    author: 'Bitwarden Inc.',
    cat: 'security',
    catName: 'Bảo mật & Mật khẩu',
    icon: '🔑',
    badge: 'Xác thực cao',
    desc: 'Quản lý mật khẩu mã hóa đầu cuối (Zero-knowledge AES-256) an toàn, tự động điền thông minh.',
    url: 'https://chromewebstore.google.com/detail/bitwarden-password-manage/nngceckbapebfimnlniiiahkandclblb'
  },
  {
    id: 'twp-translate',
    name: 'TWP - Translate Web Pages',
    author: 'Filipe Ps',
    cat: 'productivity',
    catName: 'Dịch thuật',
    icon: '🌐',
    badge: 'Thời gian thực',
    desc: 'Dịch toàn bộ trang web tức thì sang Tiếng Việt không cần tải lại trang bằng Google/Yandex Translate.',
    url: 'https://chromewebstore.google.com/detail/translate-web-pages/bifimjhdmfdpuenngbiianeedbambemi'
  },
  {
    id: 'sponsorblock',
    name: 'SponsorBlock for YouTube',
    author: 'Ajay Ramachandran',
    cat: 'productivity',
    catName: 'Năng suất',
    icon: '⏩',
    badge: 'Cộng đồng',
    desc: 'Tự động bỏ qua các đoạn quảng cáo tài trợ, intro dài, nhắc bấm like/subscribe trong video YouTube.',
    url: 'https://chromewebstore.google.com/detail/sponsorblock-for-youtube/mnjggcdmjocbbbhaepdhchncahnbgone'
  },
  {
    id: 'tampermonkey',
    name: 'Tampermonkey',
    author: 'Jan Biniok',
    cat: 'developer',
    catName: 'Lập trình & Script',
    icon: '🐵',
    badge: 'Userscripts',
    desc: 'Trình quản lý Userscript phổ biến nhất để can thiệp tùy biến giao diện và tính năng mọi trang web.',
    url: 'https://chromewebstore.google.com/detail/tampermonkey/dhdgffkkebhmkfjojejmpbldmpobfkfo'
  },
  {
    id: 'privacy-badger',
    name: 'Privacy Badger',
    author: 'Electronic Frontier Foundation (EFF)',
    cat: 'security',
    catName: 'Bảo mật',
    icon: '🦡',
    badge: 'EFF Verified',
    desc: 'Tự động học và chặn các trình theo dõi vô hình của bên thứ ba đang theo dõi hành vi của bạn.',
    url: 'https://chromewebstore.google.com/detail/privacy-badger/pkehgijcmpdhfbdbbnkijodmdjhbjlgp'
  },
  {
    id: 'singlefile',
    name: 'SingleFile',
    author: 'Gildas Lormeau',
    cat: 'productivity',
    catName: 'Năng suất & Lưu trữ',
    icon: '📄',
    badge: 'Mã nguồn mở',
    desc: 'Lưu toàn bộ trang web vào 1 file HTML duy nhất giữ nguyên ảnh, định dạng và CSS để đọc offline.',
    url: 'https://chromewebstore.google.com/detail/singlefile/mpiodijhokgodhhsofbcjdecpffjipfl'
  }
];

let visiblePasswords = new Set();
function renderPasswords() {
  const container = byId('passwords-list');
  if (!container || !state) return;
  const filter = (byId('passwords-search').value || '').toLowerCase().trim();
  const list = (state.passwords || []).filter(p =>
    !filter || p.url.toLowerCase().includes(filter) || p.username.toLowerCase().includes(filter)
  );
  if (list.length === 0) {
    container.innerHTML = `<div class="history-empty">${state.passwords?.length ? 'Không tìm thấy tài khoản phù hợp.' : 'Kho mật khẩu đang trống. Bấm "+ Thêm tài khoản" ở trên để lưu thông tin đăng nhập.'}</div>`;
    return;
  }
  container.replaceChildren();
  for (const item of list) {
    const card = make('div', undefined, 'password-card');
    const meta = make('div', undefined, 'password-meta');
    const site = make('div', item.url, 'password-site');
    const user = make('div', 'Tài khoản: ' + item.username, 'password-user');
    meta.append(site, user);

    const isVis = visiblePasswords.has(item.id);
    const valRow = make('div', undefined, 'password-val-row');
    const valBox = make('span', isVis ? item.password : '••••••••••••', 'password-masked');

    const eyeBtn = make('button', isVis ? 'Ẩn' : 'Hiện');
    eyeBtn.title = isVis ? 'Ẩn mật khẩu' : 'Hiện mật khẩu';
    eyeBtn.addEventListener('click', () => {
      if (visiblePasswords.has(item.id)) visiblePasswords.delete(item.id);
      else visiblePasswords.add(item.id);
      renderPasswords();
    });

    const copyPassBtn = make('button', 'Sao chép MK');
    copyPassBtn.title = 'Sao chép mật khẩu vào bộ nhớ tạm';
    copyPassBtn.addEventListener('click', () => {
      navigator.clipboard.writeText(item.password).then(() => toast('Đã sao chép mật khẩu!'));
    });

    const copyUserBtn = make('button', 'Sao chép User');
    copyUserBtn.title = 'Sao chép tên đăng nhập';
    copyUserBtn.addEventListener('click', () => {
      navigator.clipboard.writeText(item.username).then(() => toast('Đã sao chép tên đăng nhập!'));
    });

    const delBtn = make('button', undefined, 'history-del');
    delBtn.title = 'Xóa khỏi kho mật khẩu';
    delBtn.append(createSvgIcon('close'));
    delBtn.addEventListener('click', () => {
      if (window.confirm(`Xóa mật khẩu của ${item.url}?`)) {
        send('delete_password', { id: item.id });
      }
    });

    valRow.append(valBox, eyeBtn, copyPassBtn, copyUserBtn, delBtn);
    card.append(meta, valRow);
    container.append(card);
  }
}

let currentExtCat = 'all';
function renderExtensions() {
  const grid = byId('extensions-grid');
  if (!grid) return;
  const filter = (byId('extensions-search').value || '').toLowerCase().trim();
  const list = STORE_EXTENSIONS.filter(ext => {
    const matchCat = currentExtCat === 'all' || ext.cat === currentExtCat;
    const matchSearch = !filter || ext.name.toLowerCase().includes(filter) || ext.desc.toLowerCase().includes(filter) || ext.catName.toLowerCase().includes(filter);
    return matchCat && matchSearch;
  });
  grid.replaceChildren();
  for (const ext of list) {
    const card = make('div', undefined, 'ext-card');
    const top = make('div', undefined, 'ext-card-header');
    const icon = make('div', ext.icon, 'ext-icon');
    const info = make('div', undefined, 'ext-info');
    const h3 = make('h3', ext.name);
    const author = make('div', 'Tác giả: ' + ext.author, 'ext-author');
    const badge = make('span', ext.badge + ' · ' + ext.catName, 'ext-badge');
    info.append(h3, author, badge);
    top.append(icon, info);

    const desc = make('p', ext.desc, 'ext-desc');

    const actions = make('div', undefined, 'ext-actions');
    const installBtn = make('button', 'Cài đặt ngay ↗', 'primary');
    installBtn.title = 'Mở trang cài đặt tiện ích chính thức';
    installBtn.addEventListener('click', () => {
      send('navigate', { input: ext.url });
    });
    actions.append(installBtn);

    card.append(top, desc, actions);
    grid.append(card);
  }
}

function generateStrongPassword(len = 16) {
  const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+';
  let res = '';
  const cryptoArr = new Uint32Array(len);
  crypto.getRandomValues(cryptoArr);
  for (let i = 0; i < len; i++) {
    res += chars[cryptoArr[i] % chars.length];
  }
  return res;
}

window.applyState = next => {
  const previousActive = state?.active;
  state = next;
  const active = state.tabs.find(t => t.id === state.active);
  const isSettings = active?.url === 'papyrus://settings';
  const isHistory = active?.url === 'papyrus://history';
  const isBookmarks = active?.url === 'papyrus://bookmarks';
  const isDownloads = active?.url === 'papyrus://downloads';
  const isPasswords = active?.url === 'papyrus://passwords';
  const isExtensions = active?.url === 'papyrus://extensions';
  const isInternal = isSettings || isHistory || isBookmarks || isDownloads || isPasswords || isExtensions;

  if (!isSettings) preferencesDirty = false;
  document.body.dataset.theme = state.preferences.theme;
  document.body.classList.toggle('settings-open', isInternal);

  byId('settings-panel').hidden = !isSettings;
  byId('history-panel').hidden = !isHistory;
  byId('bookmarks-panel').hidden = !isBookmarks;
  byId('downloads-panel').hidden = !isDownloads;
  byId('passwords-panel').hidden = !isPasswords;
  byId('extensions-panel').hidden = !isExtensions;
  byId('bookmarks-row').hidden = !state.preferences.show_bookmarks;
  if (isInternal) { shieldOpen = false; byId('shield-panel').hidden = true; }
  document.body.classList.toggle('bookmarks-hidden', !state.preferences.show_bookmarks);

  if (document.activeElement !== byId('url') || state.clearing || previousActive !== state.active) {
    byId('url').value = active?.url === 'papyrus://newtab' ? '' : (active?.url || '');
  }
  byId('connection').textContent = active?.url.startsWith('https://') ? 'HTTPS' : active?.url.startsWith('http://') ? 'HTTP' : 'Nội bộ';
  byId('back').disabled = !state.can_back;
  byId('forward').disabled = !state.can_forward;
  byId('page-error').hidden = !active?.error;
  byId('page-error').textContent = active?.error || '';
  byId('adblock-counter').textContent = active?.blocked || 0;
  byId('blocked-total').textContent = state.blocked;
  byId('filter-status').textContent = state.preferences.adblock ? 'Bộ lọc đang bật' : 'Bộ lọc đang tắt';

  if (!preferencesDirty || !isSettings) setPreferenceFields(state.preferences);
  byId('version').textContent = state.version;
  byId('builtin-count').textContent = state.builtin_count;
  byId('data-location').textContent = 'Dữ liệu ứng dụng: ' + state.data_dir;
  byId('busy').hidden = !state.clearing;
  if (state.clearing) byId('clear-confirm').hidden = true;
  byId('settings-status').textContent = state.notice || '';
  byId('download-list').replaceChildren(...state.downloads.map(d => make('li', d)));

  renderTabs();
  renderBookmarks();
  if (isBookmarks) renderBookmarksManager();
  if (isHistory) renderHistory();
  if (isDownloads) renderDownloads();
  if (isPasswords) renderPasswords();
  if (isExtensions) renderExtensions();
};

window.showNotice = message => {
  toast(message);
  byId('settings-status').textContent = message;
  const statusEl = byId('update-check-status');
  if (statusEl) statusEl.textContent = message;
};

let updateData = null;
window.showUpdateToast = (info) => {
  updateData = info;
  byId('update-toast-version').textContent = 'v' + info.version;
  if (info.notes && info.notes.trim()) {
    byId('update-toast-notes').textContent = info.notes.trim().slice(0, 140);
  }
  byId('update-toast').hidden = false;
  updateHeaderHeight();
};

byId('update-toast-close').addEventListener('click', () => {
  byId('update-toast').hidden = true;
  updateHeaderHeight();
});
byId('update-toast-later').addEventListener('click', () => {
  byId('update-toast').hidden = true;
  updateHeaderHeight();
});
byId('update-toast-details').addEventListener('click', () => {
  if (updateData && updateData.url) {
    send('navigate', { input: updateData.url });
  }
  byId('update-toast').hidden = true;
  updateHeaderHeight();
});
byId('update-toast-install').addEventListener('click', () => {
  if (updateData && updateData.download_url) {
    send('navigate', { input: updateData.download_url });
  }
  byId('update-toast').hidden = true;
  updateHeaderHeight();
});

document.querySelectorAll('[data-action]').forEach(button => button.addEventListener('click', () => {
  send(button.dataset.action);
}));

document.querySelectorAll('.settings-nav a').forEach(link => link.addEventListener('click', event => {
  event.preventDefault(); byId(link.hash.slice(1)).scrollIntoView({ block: 'start' });
}));

byId('address-form').addEventListener('submit', event => {
  event.preventDefault(); byId('url').blur(); send('navigate', { input: byId('url').value });
});

byId('chrome').addEventListener('mousedown', event => {
  if (event.button !== 0) return;
  if (event.target.closest('button, input, select, a, [role="tab"], .tab-close, .window-actions, .internal-page, .dropdown-menu, #update-toast, #shield-panel, #settings-panel, #history-panel, #bookmarks-panel, #downloads-panel, #snap-layouts-menu, .snap-flyout')) {
    return;
  }
  send('drag');
});

byId('chrome').addEventListener('dblclick', event => {
  if (event.target.closest('button, input, select, a, [role="tab"], .tab-close, .window-actions, .internal-page, .dropdown-menu, #update-toast, #shield-panel, #settings-panel, #history-panel, #bookmarks-panel, #downloads-panel, #snap-layouts-menu, .snap-flyout')) {
    return;
  }
  send('maximize');
});

byId('shield-toggle').addEventListener('click', () => {
  const active = state?.tabs.find(t => t.id === state.active);
  if (['papyrus://settings', 'papyrus://history', 'papyrus://bookmarks', 'papyrus://downloads', 'papyrus://passwords', 'papyrus://extensions'].includes(active?.url)) return;
  shieldOpen = !shieldOpen;
  byId('shield-panel').hidden = !shieldOpen;
  updateHeaderHeight();
});

byId('shield-close').addEventListener('click', closeShield);

byId('menu-btn').addEventListener('click', event => {
  event.stopPropagation();
  const rect = byId('menu-btn').getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  send('show_menu', { x: Math.round(rect.left * dpr), y: Math.round(rect.bottom * dpr) });
});

// Tab Bar Right-Click Context Menu
byId('tabs-container').addEventListener('contextmenu', event => {
  event.preventDefault();
  const tabEl = event.target.closest('.tab');
  const dpr = window.devicePixelRatio || 1;
  const id = tabEl ? Number(tabEl.dataset.id) : 0;
  send('show_tab_menu', { id, x: Math.round(event.clientX * dpr), y: Math.round(event.clientY * dpr) });
});

// Suppress Chromium autoscroll on tabs during middle-click mousedown
byId('tabs-container').addEventListener('mousedown', event => {
  if (event.button === 1 && event.target.closest('.tab')) {
    event.preventDefault();
  }
});

// Middle-click tab to close
byId('tabs-container').addEventListener('auxclick', event => {
  if (event.button === 1) {
    const tabEl = event.target.closest('.tab');
    if (!tabEl) return;
    event.preventDefault();
    if (tabEl.classList.contains('pinned')) return;
    send('close_tab', { id: Number(tabEl.dataset.id) });
  }
});

// Mouse wheel horizontal scroll on tabs
byId('tabs-container').addEventListener('wheel', event => {
  if (event.deltaY !== 0) {
    event.preventDefault();
    byId('tabs-container').scrollLeft += event.deltaY;
  }
}, { passive: false });

// Double-click empty space in tab bar to open new tab
byId('tabs-container').addEventListener('dblclick', event => {
  if (event.target === byId('tabs-container')) {
    send('new_tab');
  }
});

// Drag and drop local files into Papyrus
document.addEventListener('dragover', e => e.preventDefault());
document.addEventListener('drop', e => {
  e.preventDefault();
  if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
    const f = e.dataTransfer.files[0];
    const path = f.path || f.name;
    if (path) send('navigate', { input: path });
  }
});

// Snap Layouts Flyout
const maxBtn = byId('max-btn');
if (maxBtn) {
  let snapTimer;
  maxBtn.addEventListener('contextmenu', e => {
    e.preventDefault();
    clearTimeout(snapTimer);
    const rect = maxBtn.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    send('show_snap_menu', { x: Math.round(rect.left * dpr), y: Math.round(rect.bottom * dpr) });
  });

  maxBtn.addEventListener('mouseenter', () => {
    clearTimeout(snapTimer);
    snapTimer = setTimeout(() => {
      const rect = maxBtn.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      send('show_snap_menu', { x: Math.round(rect.left * dpr), y: Math.round(rect.bottom * dpr) });
    }, 750);
  });

  maxBtn.addEventListener('mouseleave', () => {
    clearTimeout(snapTimer);
  });
}

byId('history-search').addEventListener('input', renderHistory);

byId('clear-history-btn').addEventListener('click', () => {
  if (window.confirm('Xóa toàn bộ lịch sử duyệt web của bạn? Thao tác này không thể hoàn tác.')) {
    send('clear_history');
  }
});

byId('settings-form').addEventListener('input', () => { preferencesDirty = true; });
byId('settings-form').addEventListener('submit', event => {
  event.preventDefault();
  const p = { ...state.preferences };
  for (const key of ['engine', 'startup', 'theme', 'download_dir']) p[key] = byId(key).value.trim();
  p.zoom = Number(byId('zoom').value);
  for (const key of ['show_bookmarks', 'adblock', 'block_popups']) p[key] = byId(key).checked;
  for (const key of ['allowed_hosts', 'blocked_hosts']) {
    p[key] = [...new Set(
      byId(key).value
        .split(/[\r\n]+/)
        .map(s => s.trim().toLowerCase())
        .filter(s => s && !s.startsWith('!') && !s.startsWith('#'))
    )];
  }
  preferencesDirty = false; send('save_preferences', { preferences: p });
});

byId('reset-preferences').addEventListener('click', () => {
  if (window.confirm('Khôi phục cài đặt mặc định? Ghim và dữ liệu website được giữ nguyên.')) { preferencesDirty = false; send('reset_preferences'); }
});

byId('clear-data').addEventListener('click', () => { byId('clear-confirm').hidden = false; byId('cancel-clear').focus(); updateHeaderHeight(); });
byId('cancel-clear').addEventListener('click', () => { byId('clear-confirm').hidden = true; byId('clear-data').focus(); updateHeaderHeight(); });
byId('confirm-clear').addEventListener('click', () => { byId('clear-confirm').hidden = true; updateHeaderHeight(); send('clear_data'); });

// Passwords Vault
byId('passwords-search').addEventListener('input', renderPasswords);

byId('toggle-add-pwd-btn').addEventListener('click', () => {
  const box = byId('add-pwd-box');
  box.hidden = !box.hidden;
  if (!box.hidden) {
    byId('new-pwd-url').focus();
    const active = state?.tabs.find(t => t.id === state.active);
    if (active && active.url.startsWith('http')) {
      try {
        const u = new URL(active.url);
        byId('new-pwd-url').value = u.origin;
      } catch(e) {}
    }
  }
});

byId('cancel-pwd-btn').addEventListener('click', () => {
  byId('add-pwd-box').hidden = true;
});

byId('gen-pwd-btn').addEventListener('click', () => {
  byId('new-pwd-val').value = generateStrongPassword(16);
});

byId('save-pwd-btn').addEventListener('click', () => {
  const url = byId('new-pwd-url').value.trim();
  const username = byId('new-pwd-user').value.trim();
  const password = byId('new-pwd-val').value;
  if (!url || !username || !password) {
    alert('Vui lòng nhập đầy đủ địa chỉ trang web, tên tài khoản và mật khẩu.');
    return;
  }
  send('save_password', { url, username, password });
  byId('new-pwd-url').value = '';
  byId('new-pwd-user').value = '';
  byId('new-pwd-val').value = '';
  byId('add-pwd-box').hidden = true;
});

byId('clear-passwords-btn').addEventListener('click', () => {
  if (window.confirm('Xóa toàn bộ mật khẩu đã lưu trong kho Papyrus Vault? Thao tác này không thể hoàn tác.')) {
    send('clear_passwords');
  }
});

// Extensions Store
byId('extensions-search').addEventListener('input', renderExtensions);

byId('ext-category-pills').addEventListener('click', event => {
  const pill = event.target.closest('.filter-pill');
  if (!pill) return;
  byId('ext-category-pills').querySelectorAll('.filter-pill').forEach(p => p.classList.remove('active'));
  pill.classList.add('active');
  currentExtCat = pill.dataset.cat || 'all';
  renderExtensions();
});

// Downloads Panel
byId('clear-downloads-btn')?.addEventListener('click', () => {
  if (window.confirm('Xóa toàn bộ danh sách tệp đã tải xuống?')) {
    send('clear_downloads');
  }
});

// Bookmarks Manager
byId('open-bookmarks-btn')?.addEventListener('click', () => {
  send('bookmarks');
});

byId('bookmarks-search')?.addEventListener('input', renderBookmarksManager);

byId('toggle-add-bookmark-btn')?.addEventListener('click', () => {
  const box = byId('add-bookmark-box');
  box.hidden = !box.hidden;
  if (!box.hidden) {
    byId('new-bookmark-title').focus();
    const active = state?.tabs.find(t => t.id === state.active);
    if (active && active.url.startsWith('http')) {
      byId('new-bookmark-url').value = active.url;
      byId('new-bookmark-title').value = active.title || '';
    }
  }
});

byId('cancel-bookmark-btn')?.addEventListener('click', () => {
  byId('add-bookmark-box').hidden = true;
});

byId('save-bookmark-btn')?.addEventListener('click', () => {
  const title = byId('new-bookmark-title').value.trim();
  const url = byId('new-bookmark-url').value.trim();
  if (!url) {
    alert('Vui lòng nhập địa chỉ trang web (URL).');
    return;
  }
  send('add_bookmark', { title: title || url, url });
  byId('new-bookmark-title').value = '';
  byId('new-bookmark-url').value = '';
  byId('add-bookmark-box').hidden = true;
});

byId('clear-bookmarks-btn')?.addEventListener('click', () => {
  if (window.confirm('Xóa toàn bộ dấu trang đã lưu? Thao tác này không thể hoàn tác.')) {
    send('clear_bookmarks');
  }
});

document.addEventListener('keydown', event => {
  if (event.key === 'Escape') {
    let changed = false;
    if (byId('clear-confirm') && !byId('clear-confirm').hidden) { byId('clear-confirm').hidden = true; changed = true; }
    if (shieldOpen) { shieldOpen = false; byId('shield-panel').hidden = true; changed = true; }
    if (changed) updateHeaderHeight();
    const active = state?.tabs.find(t => t.id === state.active);
    if (active && ['papyrus://settings', 'papyrus://history', 'papyrus://bookmarks', 'papyrus://downloads', 'papyrus://passwords', 'papyrus://extensions'].includes(active.url)) {
      send('close_tab', { id: active.id });
    }
  }
  if (event.ctrlKey && event.key.toLowerCase() === 'h') { event.preventDefault(); send('history'); }
  if (event.ctrlKey && event.key.toLowerCase() === 'j') { event.preventDefault(); send('downloads'); }
  if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'o') { event.preventDefault(); send('bookmarks'); }
  if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 't') { event.preventDefault(); send('reopen_tab'); }
  if (event.ctrlKey && event.key.toLowerCase() === 'm' && state) { event.preventDefault(); send('toggle_mute_tab', { id: state.active }); }
  if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'k' && state) { event.preventDefault(); send('duplicate_tab', { id: state.active }); }
  if (event.ctrlKey && event.key.toLowerCase() === 'l') { event.preventDefault(); byId('url').focus(); byId('url').select(); }
  if (event.ctrlKey && !event.shiftKey && event.key.toLowerCase() === 't') { event.preventDefault(); send('new_tab'); }
  if (event.ctrlKey && !event.shiftKey && event.key.toLowerCase() === 'n') { event.preventDefault(); send('new_window'); }
  if (event.ctrlKey && event.key.toLowerCase() === 'w' && state) { event.preventDefault(); send('close_tab', { id: state.active }); }
  if (event.ctrlKey && event.key.toLowerCase() === 'd') { event.preventDefault(); send('bookmark'); }
  if (event.ctrlKey && event.key.toLowerCase() === 'p') { event.preventDefault(); send('print'); }
  if (event.ctrlKey && (event.key === '=' || event.key === '+')) { event.preventDefault(); send('zoom_in'); }
  if (event.ctrlKey && (event.key === '-' || event.key === '_')) { event.preventDefault(); send('zoom_out'); }
  if (event.ctrlKey && event.key === '0') { event.preventDefault(); send('zoom_reset'); }
});

window.duplicateTab = function(id) {
  send('duplicate_tab', { id: id || (state ? state.active : 0) });
};
window.restoreTab = function() {
  send('restore_tab');
};
window.pinTab = function(id) {
  send('pin_tab', { id: id || (state ? state.active : 0) });
};
window.toggleMuteTab = function(id) {
  send('toggle_mute_tab', { id: id || (state ? state.active : 0) });
};

send('ready');
