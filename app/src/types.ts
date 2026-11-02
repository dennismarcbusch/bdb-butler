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
  /** Größe in der Mail (Base64), danach richtet sich `MailDraft.max_bytes`. */
  encoded_size: number;
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
  /** Höchstens so groß dürfen die Anhänge in der Mail zusammen sein (Summe von `encoded_size`). */
  max_bytes: number;
}

export interface Stammdaten {
  id: string;
  fields: Record<string, JsonValue>;
  smtp_password_set: boolean;
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
  /** Hinweise, zum Beispiel zu Einträgen, die sich nicht einspielen ließen. */
  warnings: string[];
}
