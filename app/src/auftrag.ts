// Fristen und Filter für das Auftragsbuch. Alles sind reine Funktionen, die Tageszahl hängt nur von den
// übergebenen Daten ab (`today` als `JJJJ-MM-TT`), nicht von der Uhr des Rechners.

import type { RecordView } from './types';

export const DONE_STATUS = 'Erledigt';

/// Aufträge mit einer Frist in so vielen Tagen oder weniger gelten als „bald fällig“.
export const SOON_DAYS = 7;

export type DueState = 'overdue' | 'today' | 'soon' | 'later' | 'none' | 'done';
export type OrderFilter = 'open' | 'all' | 'done';

/** Heutiges Datum (lokal) als `JJJJ-MM-TT`. */
export function todayIso(now: Date = new Date()): string {
  const y = String(now.getFullYear()).padStart(4, '0');
  const m = String(now.getMonth() + 1).padStart(2, '0');
  const d = String(now.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

function parseIso(iso: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const time = Date.UTC(y, mo - 1, d);
  const check = new Date(time);
  // Ungültige Tage wie der 31.02. fallen heraus.
  if (check.getUTCFullYear() !== y || check.getUTCMonth() !== mo - 1 || check.getUTCDate() !== d) return null;
  return time;
}

/** Anzahl Tage von `from` bis `to` (negativ, wenn `to` früher liegt). `null` bei ungültigen Daten. */
export function daysBetween(from: string, to: string): number | null {
  const a = parseIso(from);
  const b = parseIso(to);
  if (a === null || b === null) return null;
  return Math.round((b - a) / 86_400_000);
}

/** `JJJJ-MM-TT` als `TT.MM.JJJJ`. Ungültige Eingaben kommen unverändert zurück. */
export function formatDate(iso: string): string {
  return parseIso(iso) === null ? iso : `${iso.slice(8, 10)}.${iso.slice(5, 7)}.${iso.slice(0, 4)}`;
}

function text(record: RecordView, field: string): string {
  const v = record.fields[field];
  return typeof v === 'string' ? v : '';
}

export function isDone(record: RecordView): boolean {
  return text(record, 'status') === DONE_STATUS;
}

export function dueState(record: RecordView, today: string): DueState {
  if (isDone(record)) return 'done';
  const frist = text(record, 'frist');
  if (frist === '') return 'none';
  const days = daysBetween(today, frist);
  if (days === null) return 'none';
  if (days < 0) return 'overdue';
  if (days === 0) return 'today';
  return days <= SOON_DAYS ? 'soon' : 'later';
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Kurzer Text zur Frist für die Liste. */
export function dueLabel(record: RecordView, today: string): string {
  const frist = text(record, 'frist');
  const state = dueState(record, today);
  const days = frist === '' ? null : daysBetween(today, frist);
  switch (state) {
    case 'done': {
      const done = text(record, 'erledigt_am');
      return done !== '' ? `erledigt am ${formatDate(done)}` : 'erledigt';
    }
    case 'none':
      return 'ohne Frist';
    case 'overdue':
      return `überfällig seit ${plural(-(days ?? 0), 'Tag', 'Tagen')}`;
    case 'today':
      return 'heute fällig';
    case 'soon':
      return days === 1 ? 'morgen fällig' : `fällig in ${days} Tagen`;
    case 'later':
      return `Frist ${formatDate(frist)}`;
  }
}

export function filterOrders(records: RecordView[], filter: OrderFilter): RecordView[] {
  switch (filter) {
    case 'open':
      return records.filter((r) => !isDone(r));
    case 'done':
      return records.filter(isDone);
    case 'all':
      return records;
  }
}

/** Anzahl offener Aufträge mit überschrittener Frist. */
export function overdueCount(records: RecordView[], today: string): number {
  return records.filter((r) => dueState(r, today) === 'overdue').length;
}
