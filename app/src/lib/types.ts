// Mirrors `runtime::Status` in crates/engine/src/runtime.rs.

export type Mode = 'open' | 'focus';

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
  device: boolean;
  extension: boolean;
  paused: boolean;
  simulation: Simulation | null;
  window: 'clock' | 'stats' | 'image';
  focused: string | null;
  focused_site: string | null;
  open: string[];
  rules: RuleStatus[];
  keys: KeyStatus[];
}

export const modeName = (mode: Mode) => (mode === 'focus' ? 'Foco' : 'Aberto');
