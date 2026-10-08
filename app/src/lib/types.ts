// Mirrors `runtime::Status` and `config::Config` in crates/engine.

export type Mode = 'open' | 'focus';
export type WindowMode = 'clock' | 'stats' | 'image';

export interface Simulation {
  focus: number | null;
  open: number[];
}

export interface RuleStatus {
  name: string;
  kind: 'app' | 'site';
  target: string;
  mode: Mode;
  enabled: boolean;
  active: boolean;
}

export interface KeyStatus {
  number: number;
  toggled: boolean;
  label: string;
  image: string | null;
  action: string | null;
  rule: string | null;
  mode: Mode | null;
  beaten: string[];
}

export interface Status {
  config_path: string;
  config_error: string | null;
  config_revision: number;
  device: boolean;
  extension: boolean;
  paused: boolean;
  simulation: Simulation | null;
  window: WindowMode;
  focused: string | null;
  focused_site: string | null;
  open: string[];
  edge_tabs: string[];
  edge_pages: EdgePage[];
  rules: RuleStatus[];
  keys: KeyStatus[];
  screen: ScreenStatus;
}

export interface ScreenStatus {
  image: string | null;
  content: ScreenContent['type'] | '';
  rule: string | null;
}

export type ScreenContent =
  | { type: 'device_clock' }
  | { type: 'device_stats' }
  | { type: 'clock'; hour24: boolean; seconds: boolean; date: boolean }
  | { type: 'stats'; cpu: boolean; memory: boolean; gpu: boolean; network: boolean }
  | { type: 'clock_stats'; hour24: boolean; seconds: boolean }
  | { type: 'now_playing' }
  | { type: 'timer'; minutes: number }
  | { type: 'image'; icon?: string | null }
  | { type: 'text'; text: string };

export interface Screen {
  content: ScreenContent;
  background: string;
  color: string;
  accent: string;
  action?: Action | null;
}

export const SCREEN_TYPES: { type: ScreenContent['type']; label: string; hint: string }[] = [
  { type: 'clock', label: 'Relógio', hint: 'Desenhado pelo app, nas suas cores' },
  { type: 'clock_stats', label: 'Relógio + uso do PC', hint: 'Hora à esquerda, CPU, RAM e GPU à direita' },
  { type: 'stats', label: 'Uso do PC', hint: 'CPU, memória, GPU e rede com barras' },
  { type: 'now_playing', label: 'Agora tocando', hint: 'Música ou vídeo de qualquer player' },
  { type: 'timer', label: 'Cronômetro / Pomodoro', hint: 'Toque inicia e pausa; segurar zera' },
  { type: 'text', label: 'Texto', hint: 'Um texto seu, do tamanho que couber' },
  { type: 'image', label: 'Imagem', hint: 'Um ícone ou imagem sua' },
  { type: 'device_clock', label: 'Relógio do aparelho', hint: 'O relógio que o próprio D200 desenha' },
  { type: 'device_stats', label: 'Uso do PC do aparelho', hint: 'A tela simples do próprio D200' },
];

export function newContent(type: ScreenContent['type']): ScreenContent {
  switch (type) {
    case 'clock':
      return { type, hour24: true, seconds: false, date: true };
    case 'stats':
      return { type, cpu: true, memory: true, gpu: true, network: true };
    case 'clock_stats':
      return { type, hour24: true, seconds: false };
    case 'timer':
      return { type, minutes: 25 };
    case 'image':
      return { type, icon: null };
    case 'text':
      return { type, text: '' };
    default:
      return { type } as ScreenContent;
  }
}

export const newScreen = (type: ScreenContent['type'] = 'clock'): Screen => ({
  content: newContent(type),
  background: '#16171A',
  color: '#ECEDEF',
  accent: '#F0A63A',
  action: null,
});

/** The default visor, like `Config::default_screen` (older configs only have `window`). */
export function defaultScreen(config: Config): Screen {
  if (config.screen) return config.screen;
  if (config.window === 'stats') return newScreen('device_stats');
  if (config.window === 'image') return { ...newScreen('image'), content: { type: 'image', icon: config.keys['14']?.icon ?? null } };
  return newScreen('device_clock');
}

export const screenLabel = (type: string) => SCREEN_TYPES.find((s) => s.type === type)?.label ?? 'Visor';

export type MediaKey = 'play_pause' | 'next' | 'previous' | 'stop' | 'volume_up' | 'volume_down' | 'mute';

export type Action =
  | { type: 'hotkey'; keys: string }
  | { type: 'open'; target: string; args?: string | null; watch?: boolean }
  | { type: 'command'; command: string }
  | { type: 'text'; text: string }
  | { type: 'media'; key: MediaKey };

/** The second state of a two-state key; no action repeats the key's. */
export interface KeyFace {
  label: string;
  icon?: string | null;
  /** Background. */
  color: string;
  icon_color?: string | null;
  text_color?: string | null;
  border?: string | null;
  action?: Action | null;
}

export interface Key extends KeyFace {
  front?: boolean;
  toggle?: KeyFace | null;
}

export interface LabelStyle {
  show: boolean;
  size: number;
  color: string;
  align: string;
}

/** The usual color of built-in icons (icons::ICON_COLOR). */
export const ICON_COLOR = '#ECEDEF';

export interface EdgePage {
  host: string;
  url: string;
  title: string;
}

export interface RunningApp {
  exe: string;
  title: string;
  path: string;
}

/** Icons whose "off" version a two-state key most likely wants. */
const SECOND_ICON: Record<string, string> = {
  mic: 'micOff',
  camera: 'cameraOff',
  volume: 'volumeOff',
  headphones: 'headOff',
  bell: 'bellOff',
  eye: 'eyeOff',
  play: 'pause',
  record: 'stop',
};

export function secondFace(key: Key): KeyFace {
  const icon = key.icon ? (SECOND_ICON[key.icon] ?? key.icon) : null;
  const muted = icon === 'micOff' || icon === 'volumeOff' || icon === 'headOff';
  return { label: muted ? 'Mutado' : key.label, icon, color: '#3A1616', action: null };
}

export type When = { process: string } | { site: string };

export interface Rule {
  name: string;
  when: When;
  mode: Mode;
  enabled: boolean;
  keys: Record<string, Key>;
  screen?: Screen | null;
}

export interface Config {
  brightness: number;
  window: WindowMode;
  label: LabelStyle;
  keys: Record<string, Key>;
  rules: Rule[];
  screen?: Screen | null;
}

export interface Activity {
  number: number;
  label: string;
  rule: string | null;
  action: string | null;
  error: string | null;
}

export interface Glyph {
  id: string;
  /** Portuguese words to search by; the first ones name the icon. */
  label: string;
}

export const modeName = (mode: Mode) => (mode === 'focus' ? 'Foco' : 'Aberto');

/** Lowercase without accents, for searching. */
export const fold = (text: string) =>
  text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();

export const KEY_NUMBERS = Array.from({ length: 14 }, (_, i) => i + 1);

export const position = (n: number) => (n === 14 ? 'Visor' : `Linha ${Math.ceil(n / 5)} · coluna ${((n - 1) % 5) + 1}`);

export const blankKey = (): Key => ({ label: '', color: '#24262B', icon: null, action: null });

export const ACTION_TYPES: { value: Action['type']; label: string }[] = [
  { value: 'hotkey', label: 'Atalho de teclado' },
  { value: 'open', label: 'Abrir app, arquivo ou site' },
  { value: 'command', label: 'Rodar comando' },
  { value: 'text', label: 'Digitar texto' },
  { value: 'media', label: 'Mídia' },
];

export const MEDIA_KEYS: { value: MediaKey; label: string }[] = [
  { value: 'play_pause', label: 'Play/Pause' },
  { value: 'next', label: 'Próxima faixa' },
  { value: 'previous', label: 'Faixa anterior' },
  { value: 'stop', label: 'Parar' },
  { value: 'volume_up', label: 'Volume +' },
  { value: 'volume_down', label: 'Volume −' },
  { value: 'mute', label: 'Mudo' },
];

export function newAction(type: Action['type']): Action {
  switch (type) {
    case 'hotkey':
      return { type, keys: '' };
    case 'open':
      return { type, target: '' };
    case 'command':
      return { type, command: '' };
    case 'text':
      return { type, text: '' };
    case 'media':
      return { type, key: 'play_pause' };
  }
}

/** Whether an "open" target starts an app, like `Action::watched_process`
 * (URLs, URI schemes, documents and Explorer don't). */
export function opensApp(target: string): boolean {
  const t = target.trim().replace(/^"|"$/g, '');
  if (!t || t.includes('://') || (t.includes(':') && !/:[\\/]/.test(t))) return false;
  const name = (t.split(/[\\/]/).pop() ?? '').toLowerCase();
  if (name === 'explorer.exe' || name === 'explorer') return false;
  return name.endsWith('.exe') || (name.length > 0 && !name.includes('.'));
}

export function ruleTarget(rule: Rule): { kind: 'app' | 'site'; target: string } {
  return 'process' in rule.when ? { kind: 'app', target: rule.when.process } : { kind: 'site', target: rule.when.site };
}
