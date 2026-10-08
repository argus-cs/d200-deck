// Tells D200 Deck which sites are open in Edge, and selects a tab when a
// key aimed at a site is pressed. Talks only to 127.0.0.1.

// Must match `PORT` in crates/engine/src/browser.rs.
const PORT = 47820;

let socket = null;
let sendTimer = null;

function connect() {
  if (socket && socket.readyState <= WebSocket.OPEN) return;
  socket = new WebSocket(`ws://127.0.0.1:${PORT}/`);
  socket.onopen = () => sendTabs();
  socket.onmessage = (event) => handle(JSON.parse(event.data));
  socket.onclose = () => { socket = null; };
  socket.onerror = () => {}; // onclose follows; D200 Deck may simply not be running
}

async function sendTabs() {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    connect();
    return;
  }
  const all = await chrome.tabs.query({});
  const [active] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const pick = (tab) => ({ id: tab.id, url: tab.url || tab.pendingUrl || '' });
  socket.send(JSON.stringify({
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

async function handle(message) {
  if (message.type !== 'activate') return;
  let ok = false;
  try {
    const tab = await chrome.tabs.update(message.tab, { active: true });
    await chrome.windows.update(tab.windowId, { focused: true });
    ok = true;
  } catch {
    // The tab was closed in the meantime.
  }
  socket?.send(JSON.stringify({ type: 'ack', id: message.id, ok }));
}

chrome.tabs.onActivated.addListener(scheduleSend);
chrome.tabs.onUpdated.addListener((_id, change) => {
  if (change.url || change.status === 'complete') scheduleSend();
});
chrome.tabs.onRemoved.addListener(scheduleSend);
chrome.windows.onFocusChanged.addListener(scheduleSend);
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);

// Reconnects after D200 Deck restarts.
chrome.alarms.create('reconnect', { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(() => sendTabs());

// WebSocket traffic keeps the service worker alive while connected.
setInterval(() => {
  if (socket?.readyState === WebSocket.OPEN) socket.send('{"type":"keepalive"}');
}, 20000);

connect();
