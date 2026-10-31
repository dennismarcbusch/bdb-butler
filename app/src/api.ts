import type { Config, JsonValue, KindDef, RecordView, Status, SyncReport } from './types';

/** Fehler des Kerns mit stabilem Code (zum Beispiel `locked`, `wrong_secret`, `network`). */
export class BackendError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'BackendError';
    this.code = code;
  }
}

function toError(raw: unknown): BackendError {
  if (raw instanceof BackendError) return raw;
  if (raw && typeof raw === 'object' && 'code' in raw && 'message' in raw) {
    const { code, message } = raw as { code: unknown; message: unknown };
    return new BackendError(String(code), String(message));
  }
  return new BackendError('internal', typeof raw === 'string' ? raw : 'Unbekannter Fehler');
}

const inTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Ruft einen Befehl des Kerns auf: in der App per Tauri, im Browser über die Entwicklungsbrücke. */
export async function call<T>(command: string, args?: unknown): Promise<T> {
  if (inTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    try {
      return await invoke<T>('call', { command, args: args ?? null });
    } catch (e) {
      throw toError(e);
    }
  }
  let response: Response;
  try {
    response = await fetch(`/api/${command}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: args === undefined ? '' : JSON.stringify(args),
    });
  } catch {
    throw new BackendError('bridge', 'Der Kern ist nicht erreichbar');
  }
  let body: { ok: boolean; data?: T; error?: unknown };
  try {
    body = await response.json();
  } catch {
    throw new BackendError('bridge', 'Ungültige Antwort des Kerns');
  }
  if (!body.ok) throw toError(body.error);
  return body.data as T;
}

export const api = {
  status: () => call<Status>('status'),
  configGet: () => call<Config>('config_get'),
  configSet: (config: Config) => call<null>('config_set', config),
  createVault: (password: string) => call<{ recovery_key: string }>('create_vault', { password }),
  joinVault: (password: string) => call<null>('join_vault', { password }),
  unlock: (password: string) => call<null>('unlock', { password }),
  unlockRecovery: (recoveryKey: string) => call<null>('unlock_recovery', { recovery_key: recoveryKey }),
  lock: () => call<null>('lock'),
  changePassword: (oldPassword: string, newPassword: string) =>
    call<null>('change_password', { old_password: oldPassword, new_password: newPassword }),
  sync: () => call<SyncReport>('sync'),
  kinds: () => call<KindDef[]>('kinds'),
  list: (kind: string) => call<RecordView[]>('list', { kind }),
  listAll: () => call<RecordView[]>('list_all'),
  get: (id: string) => call<RecordView>('get', { id }),
  save: (kind: string, id: string | null, fields: Record<string, JsonValue>) =>
    call<{ id: string }>('save', id ? { kind, id, fields } : { kind, fields }),
  delete: (id: string) => call<null>('delete', { id }),
  conflicts: () => call<RecordView[]>('conflicts'),
};
