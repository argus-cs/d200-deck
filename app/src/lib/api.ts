import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Simulation, Status } from './types';

export const getStatus = () => invoke<Status>('get_status');
export const onStatus = (handler: (status: Status) => void) =>
  listen<Status>('status', (event) => handler(event.payload));
export const setPaused = (paused: boolean) => invoke('set_paused', { paused });
export const resend = () => invoke('resend');
export const simulate = (simulation: Simulation | null) => invoke('simulate', { simulation });
export const openConfig = () => invoke('open_config');
