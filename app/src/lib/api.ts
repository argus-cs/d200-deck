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
/** Returns the config as written to the file. */
export const saveConfig = (config: Config) => invoke<Config>('save_config', { config });
export const glyphs = () => invoke<Glyph[]>('glyphs');
export const runningApps = () => invoke<RunningApp[]>('running_apps');
export const audioOutputs = () => invoke<string[]>('audio_outputs');

/** An Iconify set, as the "more icons" window lists it (`iconify::IconSet`). */
export interface IconSet {
  prefix: string;
  name: string;
  total: number;
  category: string;
  author: string;
  license: string;
  /** The license asks for credit to the author. */
  attribution: boolean;
  /** Icons with their own colors: the key's icon color doesn't apply. */
  palette: boolean;
  samples: string[];
}

export interface IconifyIcon {
  /** Its name in the set, or its id ("mdi:bluetooth") when from several sets. */
  name: string;
  svg: string;
}

export interface IconPage {
  total: number;
  icons: IconifyIcon[];
}

export const iconifySets = () => invoke<IconSet[]>('iconify_sets');
/** One page of a set; the first call downloads it (then it is kept on disk). */
export const iconifyIcons = (prefix: string, filter: string, offset: number, limit: number) =>
  invoke<IconPage>('iconify_icons', { prefix, filter, offset, limit });
/** Icons across every set, named by id ("mdi:bluetooth"). */
export const iconifySearch = (query: string) => invoke<IconifyIcon[]>('iconify_search', { query });
/** The sets' samples by id; the ones that couldn't be had are left out. */
export const iconifySamples = (ids: string[]) => invoke<IconifyIcon[]>('iconify_samples', { ids });
/** Saves "set:icon" as an SVG for a key; returns the path to store. */
export const iconifySave = (id: string) => invoke<string>('iconify_save', { id });
export const pickImage = () => invoke<string | null>('pick_image');
export const pickFile = () => invoke<string | null>('pick_file');
export const appIcon = (path: string) => invoke<string>('app_icon', { path });
export const pickAppIcon = () => invoke<string | null>('pick_app_icon');
export const siteIcon = (url: string, host: string) => invoke<string>('site_icon', { url, host });
export const renderScreen = (screen: Screen) => invoke<string | null>('render_screen', { screen });

/** A newer version found on GitHub. */
export interface Update {
  version: string;
  notes: string | null;
}

export const getUpdate = () => invoke<Update | null>('get_update');
export const checkUpdate = () => invoke<Update | null>('check_update');
/** Downloads and installs; the app closes and opens again in the new version. */
export const installUpdate = () => invoke('install_update');
export const onUpdate = (handler: (update: Update | null) => void) =>
  listen<Update | null>('update', (event) => handler(event.payload));
/** Download progress in percent (null while the size is unknown). */
export const onUpdateProgress = (handler: (percent: number | null) => void) =>
  listen<number | null>('update-progress', (event) => handler(event.payload));

export const extensionFolder = () => invoke<string>('extension_folder');
export const openExtensionFolder = () => invoke('open_extension_folder');
export const openEdgeExtensions = () => invoke('open_edge_extensions');
