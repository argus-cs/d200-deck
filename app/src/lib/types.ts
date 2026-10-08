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
  rules: RuleStatus[];
  keys: KeyStatus[];
}

export type MediaKey = 'play_pause' | 'next' | 'previous' | 'stop' | 'volume_up' | 'volume_down' | 'mute';

export type Action =
  | { type: 'hotkey'; keys: string }
  | { type: 'open'; target: string; args?: string | null }
  | { type: 'command'; command: string }
  | { type: 'text'; text: string }
  | { type: 'media'; key: MediaKey };

export interface Key {
  label: string;
  icon?: string | null;
  color: string;
  action?: Action | null;
  front?: boolean;
}

export type When = { process: string } | { site: string };

export interface Rule {
  name: string;
  when: When;
  mode: Mode;
  enabled: boolean;
  keys: Record<string, Key>;
}

export interface Config {
  brightness: number;
  window: WindowMode;
  label: { show: boolean; size: number; color: string; align: string };
  keys: Record<string, Key>;
  rules: Rule[];
}

export const modeName = (mode: Mode) => (mode === 'focus' ? 'Foco' : 'Aberto');

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

export function ruleTarget(rule: Rule): { kind: 'app' | 'site'; target: string } {
  return 'process' in rule.when ? { kind: 'app', target: rule.when.process } : { kind: 'site', target: rule.when.site };
}
