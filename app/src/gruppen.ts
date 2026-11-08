// Schulen gliedern: nach Schulart, Ort oder Trägerschaft gruppieren und nach denselben Merkmalen filtern.
// Die Liste der Schulen und der Graph verwenden dieselbe Logik.

import type { KindDef, RecordView } from './types';

export type GroupKey = 'schulart' | 'ort' | 'traegerschaft';
/** `none`: nicht gruppieren. */
export type GroupChoice = GroupKey | 'none';

export const GROUP_CHOICES: { id: GroupChoice; label: string }[] = [
  { id: 'schulart', label: 'Schulart' },
  { id: 'ort', label: 'Ort' },
  { id: 'traegerschaft', label: 'Trägerschaft' },
  { id: 'none', label: 'Keine Gruppen' },
];

/** Bezeichnung der Gruppe für Schulen, bei denen das Merkmal fehlt. */
const MISSING: Record<GroupKey, string> = {
  schulart: 'Ohne Schulart',
  ort: 'Ohne Ort',
  traegerschaft: 'Trägerschaft offen',
};

/** Filter: leerer Text heißt „alle“. */
export interface SchoolFilter {
  schulart: string;
  ort: string;
  traegerschaft: string;
}

export const NO_FILTER: SchoolFilter = { schulart: '', ort: '', traegerschaft: '' };

export function isFiltered(filter: SchoolFilter): boolean {
  return filter.schulart !== '' || filter.ort !== '' || filter.traegerschaft !== '';
}

function text(record: RecordView, field: string): string {
  const v = record.fields[field];
  return typeof v === 'string' ? v.trim() : '';
}

/** Wert eines Merkmals, leer wenn nicht angegeben. */
export function valueOf(record: RecordView, key: GroupKey): string {
  return text(record, key);
}

/** Wert für den Filtervergleich: Orte unterscheiden sich nicht in Groß- und Kleinschreibung. */
function norm(key: GroupKey, value: string): string {
  return key === 'ort' ? value.toLowerCase() : value;
}

/** Festgelegte Reihenfolge der Auswahlwerte eines Feldes (leer bei Freitextfeldern wie dem Ort). */
export function optionsOf(kind: KindDef | undefined, key: GroupKey): string[] {
  const field = kind?.fields.find((f) => f.name === key);
  return field && field.type === 'choice' ? field.options : [];
}

/** Gruppenbezeichnung einer Schule. */
export function groupLabel(record: RecordView, key: GroupKey): string {
  return valueOf(record, key) || MISSING[key];
}

/** Ob die Schule zum Filter passt. */
export function matchesFilter(record: RecordView, filter: SchoolFilter): boolean {
  return (['schulart', 'ort', 'traegerschaft'] as const).every(
    (key) => filter[key] === '' || norm(key, valueOf(record, key)) === norm(key, filter[key]),
  );
}

/** Vorkommende Werte eines Merkmals: zuerst die festgelegten Optionen (soweit vorhanden), dann der Rest alphabetisch. */
export function distinctValues(records: RecordView[], key: GroupKey, options: string[] = []): string[] {
  const found = new Map<string, string>();
  for (const r of records) {
    const v = valueOf(r, key);
    if (v !== '' && !found.has(norm(key, v))) found.set(norm(key, v), v);
  }
  const ordered = options.filter((o) => found.has(norm(key, o)));
  const rest = [...found.entries()]
    .filter(([n]) => !options.some((o) => norm(key, o) === n))
    .map(([, v]) => v)
    .sort((a, b) => a.localeCompare(b, 'de'));
  return [...ordered, ...rest];
}

export interface Group<T> {
  label: string;
  items: T[];
}

/**
 * Teilt Schulen in Gruppen. Reihenfolge: erst die festgelegten Optionen, dann weitere Werte alphabetisch, am Ende
 * „Ohne …“. Innerhalb einer Gruppe bleibt die Reihenfolge der Eingabe. Orte, die sich nur in Groß- und
 * Kleinschreibung unterscheiden, bilden eine Gruppe.
 */
export function groupRecords(records: RecordView[], key: GroupKey, options: string[] = []): Group<RecordView>[] {
  const labels = distinctValues(records, key, options);
  const byNorm = new Map(labels.map((l) => [norm(key, l), { label: l, items: [] as RecordView[] }]));
  const missing: Group<RecordView> = { label: MISSING[key], items: [] };
  for (const r of records) {
    const v = valueOf(r, key);
    (v === '' ? missing : byNorm.get(norm(key, v))!).items.push(r);
  }
  const groups = labels.map((l) => byNorm.get(norm(key, l))!);
  return missing.items.length > 0 ? [...groups, missing] : groups;
}

/** Gruppe je Eintrags-ID, für das Layout des Graphen. */
export function groupMap(records: RecordView[], key: GroupKey, options: string[] = []): Map<string, string> {
  const out = new Map<string, string>();
  for (const g of groupRecords(records, key, options)) for (const r of g.items) out.set(r.id, g.label);
  return out;
}

/** Zusatzzeile in der Schulliste: die Merkmale, nach denen nicht schon gruppiert wird. */
export function schoolMeta(record: RecordView, grouped: GroupChoice): string {
  const parts: string[] = [];
  if (grouped !== 'ort') parts.push(valueOf(record, 'ort'));
  if (grouped !== 'schulart') parts.push(valueOf(record, 'schulart'));
  if (grouped !== 'traegerschaft' && valueOf(record, 'traegerschaft') === 'Nichtstaatlich') parts.push('nichtstaatlich');
  return parts.filter(Boolean).join(' · ');
}
