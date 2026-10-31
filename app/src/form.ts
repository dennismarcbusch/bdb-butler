import type { FieldDef, JsonValue, KindDef, RecordView } from './types';

/** Wert eines Formularfeldes: Text, Ja/Nein oder Liste von IDs. */
export type FormValue = string | boolean | string[];
export type FormValues = Record<string, FormValue>;

export function emptyValue(field: FieldDef): FormValue {
  switch (field.type) {
    case 'bool':
      return false;
    case 'ref_list':
      return [];
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
