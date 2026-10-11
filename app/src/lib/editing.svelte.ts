import { getConfig, saveConfig } from './api';
import type { Config } from './types';

/** Edits closer together than this are one undo step (typing a word). */
const UNDO_GROUP_MS = 800;
const HISTORY_LIMIT = 100;

type SaveState = 'idle' | 'saving' | 'saved';

/**
 * The config the editors work on: loaded from config.json, saved 350 ms after
 * the last edit, with undo and redo, and reloaded when the file is edited by
 * hand. Create it while a component starts: its effects belong to that one.
 */
export class ConfigEditing {
  config = $state<Config | null>(null);
  loadError = $state<string | null>(null);
  saveError = $state<string | null>(null);
  saveState = $state<SaveState>('idle');
  canUndo = $state(false);
  canRedo = $state(false);

  // What the file holds, what the editors last showed, and the history between them.
  private lastSaved = '';
  private current = '';
  private undoStack: string[] = [];
  private redoStack: string[] = [];
  private lastChangeAt = 0;
  private fromHistory = false;
  /** The file as the engine writes it, after our last load or save: when the
   * engine reloads, anything else in it is a hand edit. */
  private onDisk = '';
  private saveTimer: ReturnType<typeof setTimeout> | undefined;
  private savedTimer: ReturnType<typeof setTimeout> | undefined;
  private seenRevision = -1;
  private replaced: () => void;

  /**
   * `revision` reads the engine's config revision from the status.
   * `replaced` runs when another version takes the editors' place (a load, an
   * undo): the rule a page shows may be gone in it.
   */
  constructor(revision: () => number | undefined, replaced: () => void) {
    this.replaced = replaced;
    // Every edit lands in the undo history and is saved shortly after.
    $effect(() => {
      if (this.config) this.track(JSON.stringify($state.snapshot(this.config)));
    });
    // The engine reloaded config.json: if someone edited it by hand, show that.
    // Our own saves come back as we wrote them, however long the engine took to
    // announce them (by the clock, a slow reload wiped the undo history).
    $effect(() => {
      const current = revision();
      if (current === undefined || current === this.seenRevision) return;
      // The first one arrives with the window, right after the first load.
      const first = this.seenRevision === -1;
      this.seenRevision = current;
      if (!first) this.reloadIfEdited();
    });
  }

  async load(loaded?: Config) {
    try {
      loaded ??= await getConfig();
      const json = JSON.stringify(loaded);
      // The file's version replaces what the editors held: a save still
      // waiting would write the old one over it, and its error no longer applies.
      clearTimeout(this.saveTimer);
      this.saveError = null;
      this.saveState = 'idle';
      this.onDisk = json;
      this.lastSaved = json;
      this.current = json;
      this.undoStack = [];
      this.redoStack = [];
      this.syncHistoryButtons();
      this.config = loaded;
      this.replaced();
      this.loadError = null;
    } catch (e) {
      this.loadError = String(e);
    }
  }

  undo() {
    this.travel(this.undoStack, this.redoStack);
  }

  redo() {
    this.travel(this.redoStack, this.undoStack);
  }

  private track(json: string) {
    // Only the change an undo or redo made comes from the history: if it
    // changed nothing, the next edit must still be recorded.
    const fromHistory = this.fromHistory;
    this.fromHistory = false;
    if (json === this.current) return;
    const now = Date.now();
    if (!fromHistory) {
      if (now - this.lastChangeAt > UNDO_GROUP_MS) {
        this.undoStack.push(this.current);
        if (this.undoStack.length > HISTORY_LIMIT) this.undoStack.shift();
      }
      this.redoStack = [];
      this.lastChangeAt = now;
    }
    this.current = json;
    this.syncHistoryButtons();
    if (json === this.lastSaved) return;
    this.saveState = 'saving';
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(async () => {
      try {
        this.onDisk = JSON.stringify(await saveConfig(JSON.parse(json)));
        this.lastSaved = json;
        this.saveError = null;
        this.saveState = 'saved';
        clearTimeout(this.savedTimer);
        this.savedTimer = setTimeout(() => (this.saveState = 'idle'), 2000);
      } catch (e) {
        this.saveError = String(e);
        this.saveState = 'idle';
      }
    }, 350);
  }

  private async reloadIfEdited() {
    try {
      const loaded = await getConfig();
      if (JSON.stringify(loaded) !== this.onDisk) this.load(loaded);
    } catch {
      // Unreadable for a moment: the next revision looks again.
    }
  }

  private travel(from: string[], to: string[]) {
    const target = from.pop();
    if (target === undefined) return;
    to.push(this.current);
    this.fromHistory = true;
    this.lastChangeAt = 0;
    this.config = JSON.parse(target);
    this.replaced();
    this.syncHistoryButtons();
  }

  private syncHistoryButtons() {
    this.canUndo = this.undoStack.length > 0;
    this.canRedo = this.redoStack.length > 0;
  }
}
