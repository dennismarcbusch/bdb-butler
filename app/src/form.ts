import type { FieldDef, JsonValue, KindDef, RecordView } from './types';

/** Wert eines Formularfeldes: Text, Ja/Nein oder Liste von IDs. */
export type FormValue = string | boolean | string[];
export type FormValues = Record<string, FormValue>;

export function emptyValue(field: FieldDef): FormValue {
  switch (field.type) {
    case 'bool':
      return field.default === 'true';
    case 'ref_list':
      return [];
    case 'choice':
      return field.default ?? '';
    default:
      return '';
  }
}

export function emptyValues(kind: KindDef): FormValues {
  return Object.fromEntries(kind.fields.map((f) => [f.name, emptyValue(f)]));
}

export function valuesFromRecord(kind: KindDef, record: RecordView): FormValues {
  const values: FormValues = {};
  for (const field of kind.fields) {
    const raw = record.fields[field.name];
    if (raw === undefined || raw === null) {
      values[field.name] = emptyValue(field);
    } else if (field.type === 'bool') {
      values[field.name] = raw === true;
    } else if (field.type === 'ref_list') {
      values[field.name] = Array.isArray(raw) ? raw.filter((x): x is string => typeof x === 'string') : [];
    } else {
      values[field.name] = String(raw);
    }
  }
  return values;
}

export class FormError extends Error {}

/**
 * Übernimmt einen geänderten Eintrag ins Formular, ohne Eingaben des Nutzers zu überschreiben: Felder, die der
 * Nutzer nicht angefasst hat (Wert gleich dem bisherigen Ausgangswert), folgen dem Eintrag. Geänderte Felder
 * bleiben. Das braucht das Formular, weil sich Einträge auch von außen ändern (Status nach dem Versenden einer
 * Mail, Abgleich mit einem anderen Rechner).
 */
export function mergeIntoForm(current: FormValues, previousInitial: FormValues, nextInitial: FormValues): FormValues {
  const merged: FormValues = {};
  for (const name of new Set([...Object.keys(nextInitial), ...Object.keys(current)])) {
    const value = current[name];
    const touched = value !== undefined && JSON.stringify(value) !== JSON.stringify(previousInitial[name]);
    merged[name] = touched ? value : (nextInitial[name] ?? value ?? '');
  }
  return merged;
}

/** Namen der Felder, deren Wert sich gegenüber dem Ausgangswert geändert hat. */
export function changedFields(current: FormValues, initial: FormValues): string[] {
  return Object.keys(current).filter((name) => JSON.stringify(current[name]) !== JSON.stringify(initial[name]));
}

/** Wandelt Formularwerte in Felder für den Kern. Leere Eingaben werden zu `null` und leeren das Feld. */
export function toPayload(kind: KindDef, values: FormValues): Record<string, JsonValue> {
  const payload: Record<string, JsonValue> = {};
  for (const field of kind.fields) {
    const value = values[field.name] ?? emptyValue(field);
    switch (field.type) {
      case 'bool':
        payload[field.name] = value === true;
        break;
      case 'ref_list':
        payload[field.name] = Array.isArray(value) && value.length > 0 ? value : null;
        break;
      case 'number': {
        const text = String(value).trim().replace(',', '.');
        if (text === '') {
          payload[field.name] = null;
        } else {
          const n = Number(text);
          if (!Number.isFinite(n)) throw new FormError(`${field.label}: Zahl erwartet`);
          payload[field.name] = n;
        }
        break;
      }
      default: {
        const text = String(value).trim();
        payload[field.name] = text === '' ? null : text;
      }
    }
  }
  return payload;
}

/** Zusatzzeile in Listen: Datum, Status oder Art, je nach Typ. */
export function recordMeta(kind: KindDef, record: RecordView): string {
  const text = (name: string) => {
    const v = record.fields[name];
    return typeof v === 'string' ? v : '';
  };
  const date = (iso: string) => (/^\d{4}-\d{2}-\d{2}/.test(iso) ? `${iso.slice(8, 10)}.${iso.slice(5, 7)}.${iso.slice(0, 4)}` : '');
  switch (kind.kind) {
    case 'reise':
      return [date(text('reise_beginn')), text('status')].filter(Boolean).join(' · ');
    case 'beratung':
      return date(text('datum'));
    case 'bedarf':
      return [date(text('eingegangen_am')), text('status')].filter(Boolean).join(' · ');
    case 'fortbildung':
      return [date(text('beginn')), text('phase')].filter(Boolean).join(' · ');
    case 'protokoll':
      return [date(text('datum')), text('art')].filter(Boolean).join(' · ');
    default:
      return '';
  }
}

export function titleOf(kind: KindDef, record: RecordView): string {
  const title = record.fields[kind.title_field];
  return typeof title === 'string' && title !== '' ? title : '(ohne Titel)';
}

/** Filtert Einträge nach Suchtext in allen Textfeldern, ohne Beachtung der Groß- und Kleinschreibung. */
export function matches(record: RecordView, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === '') return true;
  return Object.values(record.fields).some((v) => typeof v === 'string' && v.toLowerCase().includes(q));
}

/** Gruppiert die Ziffern des Recovery-Keys zur besseren Lesbarkeit (er kommt schon gruppiert vom Kern). */
export function splitRecoveryKey(key: string): string[] {
  return key.split('-').filter((part) => part !== '');
}
