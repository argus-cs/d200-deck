import type { Action, AddonAction, AddonField, AddonStatus } from './types';

// The addons as the engine last reported them (`Status.addons`), for the
// editors deep down: the action picker, the key grid, the navigation.
// App.svelte keeps it current.
export const addons = $state<{ list: AddonStatus[] }>({ list: [] });

/** Goes to the Addons page; App.svelte sets it. */
export const navigation = { openAddons: () => {} };

export type AddonActionRef = Extract<Action, { type: 'addon' }>;

/** An installed addon whose addon.json loaded. */
export const findAddon = (id: string) => addons.list.find((addon) => addon.id === id && addon.state !== 'invalid') ?? null;

/** What an addon's action is, if the addon is installed and offers it. */
export function findAction(action: AddonActionRef): { addon: AddonStatus; def: AddonAction } | null {
  const addon = findAddon(action.addon);
  const def = addon?.actions.find((a) => a.id === action.action);
  return addon && def ? { addon, def } : null;
}

/** The actions of the enabled addons, for the action picker. */
export const enabledActions = () =>
  addons.list.filter((addon) => addon.enabled && addon.state !== 'invalid').flatMap((addon) => addon.actions.map((def) => ({ addon, def })));

/** How the action picker names a choice: "addon:<id>:<action>". */
export const actionValue = (addon: string, action: string) => `addon:${addon}:${action}`;

/** What a new key with this action holds: each field's default. */
export function newAddonAction(addon: AddonStatus, def: AddonAction): AddonActionRef {
  const settings: Record<string, unknown> = {};
  for (const field of def.fields) {
    const value = field.default ?? (field.type === 'checkbox' ? false : undefined);
    if (value !== undefined) settings[field.id] = value;
  }
  return { type: 'addon', addon: addon.id, action: def.id, settings };
}

/** A field's value as the editor shows it. */
export function fieldValue(field: AddonField, settings: Record<string, unknown>): unknown {
  return settings[field.id] ?? field.default ?? (field.type === 'checkbox' ? false : '');
}

/** Why an addon's key doesn't work now, or null when it does. */
export function addonProblem(action: AddonActionRef): string | null {
  const addon = findAddon(action.addon);
  if (!addon) return `O addon ${action.addon} não está instalado. A tecla guarda o que foi configurado e volta a funcionar quando ele for instalado.`;
  if (!addon.actions.some((a) => a.id === action.action)) return `O addon ${addon.name} não tem mais a ação "${action.action}".`;
  if (!addon.enabled) return `O addon ${addon.name} está desligado. A tecla fica escura e não faz nada até ele ser ligado.`;
  if (addon.state === 'restarting') return `O addon ${addon.name} fechou e está abrindo de novo.`;
  if (addon.state === 'stopped') return `O addon ${addon.name} parou. A tecla fica escura até ele voltar a rodar.`;
  return null;
}

/** For the navigation: how many run, and whether one stopped. */
export function addonsSummary(): { running: number; stopped: number; total: number } {
  const valid = addons.list.filter((addon) => addon.state !== 'invalid');
  return {
    running: valid.filter((addon) => addon.state === 'running').length,
    stopped: valid.filter((addon) => addon.enabled && (addon.state === 'stopped' || addon.state === 'restarting')).length,
    total: valid.length,
  };
}
