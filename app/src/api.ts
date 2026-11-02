import type { AuditEntry, Config, JsonValue, KindDef, MailDraft, McpState, RecordView, Stammdaten, Status, SyncReport } from './types';

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

/** Liest eine ausgewählte Datei als Base64 (ohne Präfix) für den Transport zum Kern. */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? '');
      resolve(result.slice(result.indexOf(',') + 1));
    };
    reader.onerror = () => reject(new BackendError('file', 'Die Datei konnte nicht gelesen werden'));
    reader.readAsDataURL(file);
  });
}

/** Öffnet eine Datei aus dem Ordner für PDFs und Anhänge: in der App im Standardprogramm, im Browser in einem neuen Tab. */
export async function openFile(path: string): Promise<void> {
  if (inTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    try {
      await invoke('open_file', { path });
    } catch (e) {
      throw toError(e);
    }
    return;
  }
  const { name, data } = await call<{ name: string; data: string }>('file_read', { path });
  const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
  const type = name.toLowerCase().endsWith('.pdf') ? 'application/pdf' : 'application/octet-stream';
  window.open(URL.createObjectURL(new Blob([bytes], { type })), '_blank');
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
  /** Setzt diesen Rechner zurück und befüllt den Server neu, danach normaler Abgleich. Nur nach Bestätigung. */
  serverRefill: () => call<SyncReport>('server_refill'),
  kinds: () => call<KindDef[]>('kinds'),
  list: (kind: string) => call<RecordView[]>('list', { kind }),
  listAll: () => call<RecordView[]>('list_all'),
  get: (id: string) => call<RecordView>('get', { id }),
  save: (kind: string, id: string | null, fields: Record<string, JsonValue>) =>
    call<{ id: string }>('save', id ? { kind, id, fields } : { kind, fields }),
  delete: (id: string) => call<null>('delete', { id }),
  conflicts: () => call<RecordView[]>('conflicts'),
  stammdatenGet: () => call<Stammdaten>('stammdaten_get'),
  stammdatenSave: (fields: Record<string, JsonValue>) => call<null>('stammdaten_save', { fields }),
  smtpPasswordSet: (password: string) => call<null>('smtp_password_set', { password }),
  reiseAntrag: (id: string) => call<{ path: string }>('reise_antrag', { id }),
  reiseAbrechnung: (id: string) => call<{ path: string }>('reise_abrechnung', { id }),
  reiseR002: (id: string) => call<{ path: string }>('reise_r002', { id }),
  reiseTicketpreis: (id: string) => call<{ candidates: { name: string; price: number | null }[] }>('reise_ticketpreis', { id }),
  reiseFormular: (id: string, name: string, data: string) => call<{ path: string }>('reise_formular', { id, name, data }),
  reiseGenehmigung: (id: string, name: string, data: string) => call<{ path: string }>('reise_genehmigung', { id, name, data }),
  anhangAdd: (reise: string, art: string, name: string, data: string) => call<{ id: string }>('anhang_add', { reise, art, name, data }),
  anhangDelete: (id: string) => call<null>('anhang_delete', { id }),
  mailDraft: (id: string, kind: 'antrag' | 'abrechnung') => call<MailDraft>('mail_draft', { id, kind }),
  mailSend: (id: string, kind: 'antrag' | 'abrechnung', subject: string, body: string, attachments: string[]) =>
    call<null>('mail_send', { id, kind, subject, body, attachments }),
  reportSave: (name: string, content: string) => call<{ path: string }>('report_save', { name, content }),
  mcpGet: () => call<McpState>('mcp_get'),
  mcpSet: (enabled: boolean, port: number, write: boolean, allowPersonal: string[]) =>
    call<McpState>('mcp_set', { enabled, port, write, allow_personal: allowPersonal }),
  mcpNewToken: () => call<McpState>('mcp_new_token'),
  mcpAudit: (limit = 30) => call<AuditEntry[]>('mcp_audit', { limit }),
};
