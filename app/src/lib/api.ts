import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Activity, Config, Glyph, RunningApp, Screen, Simulation, Status } from './types';

export const getStatus = () => invoke<Status>('get_status');
export const onStatus = (handler: (status: Status) => void) =>
  listen<Status>('status', (event) => handler(event.payload));
export const onActivity = (handler: (activity: Activity) => void) =>
  listen<Activity>('activity', (event) => handler(event.payload));
export const setPaused = (paused: boolean) => invoke('set_paused', { paused });
export const resend = () => invoke('resend');
export const simulate = (simulation: Simulation | null) => invoke('simulate', { simulation });
export const openConfig = () => invoke('open_config');

export const getConfig = () => invoke<Config>('get_config');
export const saveConfig = (config: Config) => invoke('save_config', { config });
export const glyphs = () => invoke<Glyph[]>('glyphs');
export const runningApps = () => invoke<RunningApp[]>('running_apps');
export const pickImage = () => invoke<string | null>('pick_image');
export const pickFile = () => invoke<string | null>('pick_file');
export const appIcon = (path: string) => invoke<string>('app_icon', { path });
export const pickAppIcon = () => invoke<string | null>('pick_app_icon');
export const siteIcon = (url: string, host: string) => invoke<string>('site_icon', { url, host });
export const renderScreen = (screen: Screen) => invoke<string | null>('render_screen', { screen });

export const extensionFolder = () => invoke<string>('extension_folder');
export const openExtensionFolder = () => invoke('open_extension_folder');
export const openEdgeExtensions = () => invoke('open_edge_extensions');
