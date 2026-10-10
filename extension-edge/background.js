// Tells D200 Deck which sites are open in Edge, and selects a tab when a
// key aimed at a site is pressed. Talks only to 127.0.0.1.

// Must match `PORT` in crates/engine/src/browser.rs.
const PORT = 47820;

let socket = null;
let sendTimer = null;

function connect() {
  if (socket && socket.readyState <= WebSocket.OPEN) return;
  // The handlers hold their own socket: a closing one must not drop the newer
  // socket that replaced it.
  const ws = new WebSocket(`ws://127.0.0.1:${PORT}/`);
  socket = ws;
  ws.onopen = () => sendTabs();
  ws.onmessage = (event) => {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    if (message && typeof message === 'object') handle(ws, message);
  };
  ws.onclose = () => {
    if (socket === ws) socket = null;
  };
  ws.onerror = () => {}; // onclose follows; D200 Deck may simply not be running
}

const isOpen = (ws) => ws?.readyState === WebSocket.OPEN;

async function sendTabs() {
  const ws = socket;
  if (!isOpen(ws)) {
    connect();
    return;
  }
  const all = await chrome.tabs.query({});
  const [active] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  // It may have closed, or been replaced, while Edge answered.
  if (ws !== socket || !isOpen(ws)) return;
  const pick = (tab) => ({ id: tab.id, url: tab.url || tab.pendingUrl || '', title: tab.title || '' });
  ws.send(JSON.stringify({
    type: 'tabs',
    active: active ? pick(active) : null,
    open: all.map(pick).filter((tab) => tab.url),
  }));
}

// Several tab events fire together (open, load, select): send once.
function scheduleSend() {
  clearTimeout(sendTimer);
  sendTimer = setTimeout(sendTabs, 100);
}

// Answers go back on the socket that asked.
async function handle(ws, message) {
  if (message.type === 'favicon' && typeof message.url === 'string') return sendFavicon(ws, message);
  if (message.type !== 'activate' || !Number.isInteger(message.tab)) return;
  let ok = false;
  try {
    const tab = await chrome.tabs.update(message.tab, { active: true });
    await chrome.windows.update(tab.windowId, { focused: true });
    ok = true;
  } catch {
    // The tab was closed in the meantime.
  }
  if (isOpen(ws)) ws.send(JSON.stringify({ type: 'ack', id: message.id, ok }));
}

// The icon Edge already keeps for a page (the "favicon" permission): the
// site itself is not contacted.
async function sendFavicon(ws, message) {
  let data = null;
  try {
    const url = chrome.runtime.getURL(`/_favicon/?pageUrl=${encodeURIComponent(message.url)}&size=128`);
    const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    data = btoa(binary);
  } catch {
    // No icon for that page.
  }
  if (isOpen(ws)) ws.send(JSON.stringify({ type: 'favicon', id: message.id, data }));
}

chrome.tabs.onActivated.addListener(scheduleSend);
chrome.tabs.onUpdated.addListener((_id, change) => {
  if (change.url || change.status === 'complete') scheduleSend();
});
chrome.tabs.onRemoved.addListener(scheduleSend);
chrome.windows.onFocusChanged.addListener(scheduleSend);
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);

// Reconnects after D200 Deck restarts. Created once: creating it on every
// service worker start would restart its countdown each time.
chrome.alarms.get('reconnect').then((alarm) => {
  if (!alarm) chrome.alarms.create('reconnect', { periodInMinutes: 0.5 });
});
chrome.alarms.onAlarm.addListener(() => sendTabs());

// WebSocket traffic keeps the service worker alive while connected.
setInterval(() => {
  if (isOpen(socket)) socket.send('{"type":"keepalive"}');
}, 20000);

connect();
