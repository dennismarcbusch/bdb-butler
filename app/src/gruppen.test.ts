import { describe, expect, it } from 'vitest';
import {
  NO_FILTER,
  distinctValues,
  groupMap,
  groupRecords,
  isFiltered,
  matchesFilter,
  schoolMeta,
} from './gruppen';
import type { RecordView } from './types';

const school = (id: string, fields: RecordView['fields']): RecordView => ({ id, kind: 'schule', fields });
const ARTEN = ['Grundschule', 'Mittelschule', 'Gymnasium'];

const schools = [
  school('a', { name: 'A', schulart: 'Gymnasium', ort: 'Passau', traegerschaft: 'Staatlich' }),
  school('b', { name: 'B', schulart: 'Grundschule', ort: 'passau', traegerschaft: 'Nichtstaatlich' }),
  school('c', { name: 'C', schulart: 'Gymnasium', ort: 'Deggendorf' }),
  school('d', { name: 'D', ort: 'Aachen', traegerschaft: 'Staatlich' }),
  school('e', { name: 'E', schulart: 'Berufsschule', ort: ' Passau ' }),
];

describe('groupRecords', () => {
  it('ordnet Schularten nach der festgelegten Reihenfolge, unbekannte danach, „Ohne“ zuletzt', () => {
    const groups = groupRecords(schools, 'schulart', ARTEN);
    expect(groups.map((g) => [g.label, g.items.map((r) => r.id)])).toEqual([
      ['Grundschule', ['b']],
      ['Gymnasium', ['a', 'c']],
      ['Berufsschule', ['e']],
      ['Ohne Schulart', ['d']],
    ]);
  });

  it('sortiert Orte alphabetisch und fasst Schreibweisen zusammen', () => {
    const groups = groupRecords(schools, 'ort');
    expect(groups.map((g) => g.label)).toEqual(['Aachen', 'Deggendorf', 'Passau']);
    expect(groups[2]!.items.map((r) => r.id)).toEqual(['a', 'b', 'e']);
  });

  it('trennt staatlich und nichtstaatlich, Schulen ohne Angabe stehen am Ende', () => {
    const groups = groupRecords(schools, 'traegerschaft', ['Staatlich', 'Nichtstaatlich']);
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([
      ['Staatlich', 2],
      ['Nichtstaatlich', 1],
      ['Trägerschaft offen', 2],
    ]);
  });

  it('liefert keine leeren Gruppen', () => {
    expect(groupRecords([], 'ort')).toEqual([]);
    expect(groupRecords([school('x', { name: 'X' })], 'ort').map((g) => g.label)).toEqual(['Ohne Ort']);
  });
});

describe('groupMap', () => {
  it('ordnet jeder Schule ihre Gruppe zu', () => {
    const m = groupMap(schools, 'schulart', ARTEN);
    expect(m.get('a')).toBe('Gymnasium');
    expect(m.get('d')).toBe('Ohne Schulart');
    expect(m.size).toBe(schools.length);
  });
});

describe('Filter', () => {
  it('lässt alle durch, wenn nichts gewählt ist', () => {
    expect(isFiltered(NO_FILTER)).toBe(false);
    expect(schools.every((s) => matchesFilter(s, NO_FILTER))).toBe(true);
  });

  it('verbindet die Merkmale mit „und“, Orte ohne Beachtung der Schreibweise', () => {
    const ids = (f: Partial<typeof NO_FILTER>) => schools.filter((s) => matchesFilter(s, { ...NO_FILTER, ...f })).map((s) => s.id);
    expect(ids({ traegerschaft: 'Staatlich' })).toEqual(['a', 'd']);
    expect(ids({ ort: 'Passau' })).toEqual(['a', 'b', 'e']);
    expect(ids({ ort: 'Passau', schulart: 'Gymnasium' })).toEqual(['a']);
    expect(ids({ traegerschaft: 'Nichtstaatlich', schulart: 'Gymnasium' })).toEqual([]);
  });
});

describe('distinctValues und schoolMeta', () => {
  it('liefert vorkommende Werte, Optionen zuerst', () => {
    expect(distinctValues(schools, 'schulart', ARTEN)).toEqual(['Grundschule', 'Gymnasium', 'Berufsschule']);
    expect(distinctValues(schools, 'ort')).toEqual(['Aachen', 'Deggendorf', 'Passau']);
  });

  it('zeigt in der Liste nur, wonach nicht gruppiert ist', () => {
    const b = schools[1]!;
    expect(schoolMeta(b, 'schulart')).toBe('passau · nichtstaatlich');
    expect(schoolMeta(b, 'ort')).toBe('Grundschule · nichtstaatlich');
    expect(schoolMeta(b, 'traegerschaft')).toBe('passau · Grundschule');
    expect(schoolMeta(b, 'none')).toBe('passau · Grundschule · nichtstaatlich');
  });
});
