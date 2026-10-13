// Retakes the README pictures in docs/images with the real app: the demo
// config in a throwaway %APPDATA% (the real config is never touched), the
// window driven over the WebView2 DevTools protocol, and the GIF put
// together by ffmpeg.
//
// From the repo root, with D200 Deck closed (the app opens only once) and
// ffmpeg in the PATH:
//   node docs/screenshots/capture.mjs
// While it runs, the D200 shows the demo layout; open D200 Deck again after.

import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const OUT = path.join(ROOT, 'docs', 'images');
const DEVTOOLS_PORT = 9222;
const VIEWPORT = { width: 1280, height: 800, deviceScaleFactor: 1.5, mobile: false };
const THEMES = ['dark', 'light'];
/** Iconify icons the demo config's keys use (icons/<prefix>-<name>.svg). */
const ICONIFY = ['logos:microsoft-edge', 'logos:spotify-icon', 'logos:discord-icon'];
/** Rules by their place in demo-config.json. */
const TEAMS = 0;
const DISCORD = 1;
const YOUTUBE = 2;
const VSCODE = 3;
const NOTHING = { focus: null, open: [] };

/** Each picture of the window: what the simulator says is open, and the clicks. */
const SHOTS = [
  { name: 'live', simulation: { focus: TEAMS, open: [] }, steps: [] },
  { name: 'rule', simulation: { focus: TEAMS, open: [] }, steps: [['nav', 'Teams'], ['key', 1]] },
  { name: 'windows', simulation: NOTHING, steps: [['nav', 'Layout padrão'], ['key', 9], ['scroll', 'Ao apertar']] },
  { name: 'folder', simulation: NOTHING, steps: [['nav', 'Layout padrão'], ['open', 4], ['key', 3]] },
  { name: 'visor', simulation: NOTHING, steps: [['nav', 'Layout padrão'], ['key', 14]] },
  { name: 'icons', simulation: NOTHING, steps: [['nav', 'Layout padrão'], ['key', 7], ['button', 'Mais ícones'], ['set', 'SVG Logos']] },
];

/** The GIF's frames: the keys as each context leaves them. */
const FRAMES = [NOTHING, { focus: TEAMS, open: [] }, { focus: YOUTUBE, open: [] }, { focus: DISCORD, open: [] }, { focus: null, open: [VSCODE] }];
const FRAME_SECONDS = 1.8;

const SCREEN = { background: '#16171A', color: '#ECEDEF', accent: '#F0A63A' };
/** Visor examples, drawn by the engine like on the device. */
const VISORS = [
  ['clock', { content: { type: 'clock', hour24: true, seconds: false, date: true } }],
  ['stats', { content: { type: 'stats', cpu: true, memory: true, gpu: true, network: true } }],
  ['pomodoro', { content: { type: 'timer', minutes: 25 } }],
  ['text', { content: { type: 'text', text: 'Em call\nnão perturbe' }, background: '#1C1F3B', accent: '#5865F2' }],
];

// Node 20 has WebSocket only behind a flag.
if (typeof WebSocket === 'undefined') {
  const child = spawn(process.execPath, ['--experimental-websocket', ...process.argv.slice(1)], { stdio: 'inherit' });
  child.on('exit', (code) => process.exit(code ?? 1));
} else {
  await main().catch((error) => {
    console.error(`Erro: ${error.message}`);
    process.exitCode = 1;
  });
}

async function main() {
  const running = execFileSync('tasklist', ['/FI', 'IMAGENAME eq d200-deck.exe', '/NH'], { encoding: 'utf8' });
  if (running.toLowerCase().includes('d200-deck.exe')) throw new Error('feche o D200 Deck antes (Sair, na bandeja): o app só abre uma vez.');

  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'd200deck-shots-'));
  fs.mkdirSync(path.join(temp, 'D200Deck'));
  fs.copyFileSync(path.join(HERE, 'demo-config.json'), path.join(temp, 'D200Deck', 'config.json'));
  fs.mkdirSync(OUT, { recursive: true });

  console.log('Abrindo o app (a primeira compilação pode levar alguns minutos)…');
  const log = fs.openSync(path.join(temp, 'tauri-dev.log'), 'w');
  const app = spawn('npx tauri dev', {
    cwd: path.join(ROOT, 'app'),
    shell: true,
    stdio: ['ignore', log, log],
    env: {
      ...process.env,
      // The engine finds config, icons, caches and log through it.
      APPDATA: temp,
      // localStorage too: the extension guide, remembered choices.
      WEBVIEW2_USER_DATA_FOLDER: path.join(temp, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${DEVTOOLS_PORT}`,
    },
  });

  let ok = false;
  try {
    const cdp = await connect(await findPage(app));
    await prepare(cdp);
    for (const theme of THEMES) {
      await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] });
      for (const shot of SHOTS) await takeShot(cdp, shot, theme);
      await takeGif(cdp, theme, temp);
    }
    await takeVisors(cdp);
    await page(cdp, (simulation) => window.__TAURI_INTERNALS__.invoke('simulate', { simulation }), null);
    cdp.close();
    ok = true;
  } finally {
    try {
      execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {
      // Already gone.
    }
    if (ok) {
      // The WebView2 processes take a moment to let go of their files.
      await sleep(3000);
      try {
        fs.rmSync(temp, { recursive: true, force: true, maxRetries: 10, retryDelay: 1000 });
      } catch {
        console.log(`Não deu para apagar ${temp}; pode apagar depois.`);
      }
    } else {
      console.error(`O log do tauri dev e a config de teste ficaram em ${temp}`);
    }
  }
  console.log(`Pronto: imagens em ${path.relative(ROOT, OUT)}`);
}

/** Saves the demo's Iconify icons through the app, then makes it draw them. */
async function prepare(cdp) {
  await cdp.send('Emulation.setDeviceMetricsOverride', VIEWPORT);
  await waitFor(cdp, () => !!document.querySelector('.shell'));
  await page(
    cdp,
    async (ids) => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      for (const id of ids) await invoke('iconify_save', { id });
      // The keys were drawn before the icons existed.
      await invoke('save_config', { config: await invoke('get_config') });
      localStorage.setItem('d200deck.extensionGuideDismissed', '1');
    },
    ICONIFY,
  );
  await sleep(2000);
}

async function takeShot(cdp, shot, theme) {
  await reload(cdp);
  await simulate(cdp, shot.simulation);
  for (const [step, arg] of shot.steps) {
    await page(cdp, uiStep, step, arg);
    await sleep(step === 'set' ? 6000 : 700);
  }
  if (shot.name === 'icons') await waitFor(cdp, () => document.querySelectorAll('dialog .grid .tile').length > 0, 60000);
  await waitFor(cdp, drawn, 30000);
  await page(cdp, hideSimulationBanner);
  await sleep(300);
  await screenshot(cdp, `${shot.name}-${theme}.png`);
}

/** Clicks in the page; runs there, so it can only use its arguments. */
function uiStep(step, arg) {
  const find = (selector, text) => [...document.querySelectorAll(selector)].find((el) => el.textContent.includes(text));
  const click = (el, what) => {
    if (!el) throw new Error(`não achei ${what}`);
    el.click();
  };
  switch (step) {
    case 'nav':
      return click(find('.nav-item', arg), `"${arg}" na barra lateral`);
    case 'key':
      return click(document.querySelector(arg === 14 ? '.deck button[aria-label^="Visor"]' : `.deck button[aria-label^="Tecla ${arg}:"]`), `a tecla ${arg}`);
    case 'open': {
      const key = document.querySelector(`.deck button[aria-label^="Tecla ${arg}:"]`);
      if (!key) throw new Error(`não achei a tecla ${arg}`);
      key.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
      return;
    }
    case 'button':
      return click(find('button', arg), `o botão "${arg}"`);
    case 'scroll': {
      const field = find('.field', arg);
      if (!field) throw new Error(`não achei "${arg}"`);
      field.scrollIntoView({ block: 'start' });
      return;
    }
    case 'set': {
      const set = find('dialog .set', arg);
      click(set, `a coleção "${arg}"`);
      set.scrollIntoView({ block: 'center' });
      return;
    }
  }
}

/** Key, visor and icon previews come from the engine a moment after the page. */
function drawn() {
  const keys = [...document.querySelectorAll('.deck .key')].every(
    (key) => key.querySelector('img, .time, .stats') || key.classList.contains('empty') || key.querySelector('.hint:not(.small-hint)'),
  );
  const glyphs = [...document.querySelectorAll('.glyph')].every((glyph) => glyph.querySelector('img'));
  const images = [...document.images].every((img) => img.complete);
  return keys && glyphs && images;
}

/** The banner only says the simulator is on, which is how these are taken. */
function hideSimulationBanner() {
  for (const banner of document.querySelectorAll('.banner')) {
    if (banner.textContent.includes('Simulando contexto')) banner.style.display = 'none';
  }
}

/** The live view's deck in each context, as a looping GIF. */
async function takeGif(cdp, theme, temp) {
  await reload(cdp);
  await page(cdp, () => {
    const style = document.createElement('style');
    style.textContent = '.deck .key.selected { box-shadow: none !important; }';
    document.head.append(style);
  });
  const frames = path.join(temp, `frames-${theme}`);
  fs.mkdirSync(frames, { recursive: true });
  for (const [i, simulation] of FRAMES.entries()) {
    await simulate(cdp, simulation);
    await page(cdp, hideSimulationBanner);
    await waitFor(cdp, drawn, 30000);
    const clip = await page(cdp, () => {
      const rects = ['.chips', '.deck'].map((s) => document.querySelector(s).getBoundingClientRect());
      const pad = 16;
      const x = Math.min(...rects.map((r) => r.left)) - pad;
      const y = Math.min(...rects.map((r) => r.top)) - pad;
      const right = Math.max(...rects.map((r) => r.right)) + pad;
      const bottom = Math.max(...rects.map((r) => r.bottom)) + pad;
      return { x, y, width: right - x, height: bottom - y, scale: 1 };
    });
    const { data } = await cdp.send('Page.captureScreenshot', { format: 'png', clip });
    fs.writeFileSync(path.join(frames, `${i}.png`), Buffer.from(data, 'base64'));
  }
  const gif = path.join(OUT, `rules-${theme}.gif`);
  execFileSync('ffmpeg', [
    '-y', '-loglevel', 'error',
    '-framerate', `1/${FRAME_SECONDS}`, '-i', path.join(frames, '%d.png'),
    '-vf', 'split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse',
    '-loop', '0', gif,
  ]);
  console.log(`  ${path.relative(ROOT, gif)}`);
}

async function takeVisors(cdp) {
  for (const [name, screen] of VISORS) {
    const image = await page(cdp, (screen) => window.__TAURI_INTERNALS__.invoke('render_screen', { screen }), { ...SCREEN, ...screen });
    const file = path.join(OUT, `visor-${name}.png`);
    fs.writeFileSync(file, Buffer.from(image.split(',')[1], 'base64'));
    console.log(`  ${path.relative(ROOT, file)}`);
  }
}

async function simulate(cdp, simulation) {
  await page(cdp, (simulation) => window.__TAURI_INTERNALS__.invoke('simulate', { simulation }), simulation);
  // The engine draws the keys and sends a new status.
  await sleep(1500);
}

async function reload(cdp) {
  await page(cdp, () => setTimeout(() => location.reload()));
  await sleep(1000);
  await waitFor(cdp, () => !!document.querySelector('.shell .deck'));
  await sleep(1000);
}

async function screenshot(cdp, name) {
  const { data } = await cdp.send('Page.captureScreenshot', { format: 'png' });
  const file = path.join(OUT, name);
  fs.writeFileSync(file, Buffer.from(data, 'base64'));
  console.log(`  ${path.relative(ROOT, file)}`);
}

async function findPage(app) {
  const deadline = Date.now() + 15 * 60_000;
  while (Date.now() < deadline) {
    if (app.exitCode !== null) throw new Error('o tauri dev terminou antes de abrir a janela.');
    try {
      const targets = await (await fetch(`http://127.0.0.1:${DEVTOOLS_PORT}/json`)).json();
      const target = targets.find((t) => t.type === 'page' && t.url.startsWith('http://localhost:1430'));
      if (target) return target.webSocketDebuggerUrl;
    } catch {
      // Not up yet.
    }
    await sleep(1000);
  }
  throw new Error('a janela do app não apareceu em 15 minutos.');
}

/** A DevTools protocol session: `send(method, params)` resolves with the result. */
async function connect(url) {
  const ws = new WebSocket(url);
  const waiting = new Map();
  let next = 0;
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    const call = waiting.get(message.id);
    if (!call) return;
    waiting.delete(message.id);
    if (message.error) call.reject(new Error(message.error.message));
    else call.resolve(message.result);
  };
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });
  return {
    send: (method, params = {}) =>
      new Promise((resolve, reject) => {
        const id = ++next;
        waiting.set(id, { resolve, reject });
        ws.send(JSON.stringify({ id, method, params }));
      }),
    close: () => ws.close(),
  };
}

/** Runs `fn` in the page with `args` (JSON) and returns what it returns. */
async function page(cdp, fn, ...args) {
  const expression = `(${fn})(...${JSON.stringify(args)})`;
  const { result, exceptionDetails } = await cdp.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  return result.value;
}

async function waitFor(cdp, check, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      if (await page(cdp, check)) return;
    } catch {
      // The page may be reloading.
    }
    await sleep(250);
  }
  throw new Error(`esperei demais por ${check}`);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
