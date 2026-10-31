// Typen der Schnittstelle zum Kern (siehe core/src/api.rs und core/src/schema.rs).

export type FieldType =
  | { type: 'text' }
  | { type: 'long_text' }
  | { type: 'markdown' }
  | { type: 'date' }
  | { type: 'date_time' }
  | { type: 'number' }
  | { type: 'bool' }
  | { type: 'choice'; options: string[] }
  | { type: 'ref'; kind: string }
  | { type: 'ref_list'; kind: string };

export type FieldDef = FieldType & { name: string; label: string; required: boolean; default?: string; internal?: boolean };

export interface KindDef {
  kind: string;
  label: string;
  label_plural: string;
  title_field: string;
  sort_field: string;
  sort_desc: boolean;
  personal: boolean;
  hidden?: boolean;
  singleton?: boolean;
  fields: FieldDef[];
}

export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

export interface RecordView {
  id: string;
  kind: string;
  fields: Record<string, JsonValue>;
}

export interface Status {
  configured: boolean;
  has_vault: boolean;
  unlocked: boolean;
}

export interface Config {
  server_url: string;
  token: string;
  attachments_dir: string;
}

export interface MailDraftAttachment {
  key: string;
  name: string;
  size: number;
  selected: boolean;
  required: boolean;
}

export interface MailDraft {
  to: string;
  from: string;
  subject: string;
  body: string;
  attachments: MailDraftAttachment[];
  already_sent: string | null;
}

export interface Stammdaten {
  id: string;
  fields: Record<string, JsonValue>;
  smtp_password_set: boolean;
}

export interface ImportReport {
  stammdaten: string;
  trips_imported: number;
  trips_skipped: number;
  attachments_imported: number;
  attachments_skipped: number;
  files_copied: number;
  warnings: string[];
  errors: string[];
}

export interface McpState {
  enabled: boolean;
  port: number;
  token: string;
  write: boolean;
  allow_personal: string[];
  running: boolean;
  url: string | null;
}

export interface AuditEntry {
  t: number;
  tool: string;
  kind?: string;
  id?: string;
  fields?: string[];
  ok: boolean;
  error?: string;
}

export interface SyncReport {
  pulled: number;
  pushed: number;
  merged: number;
  conflicts: number;
}
