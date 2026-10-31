import { describe, expect, it } from 'vitest';
import { changedFields, emptyValues, FormError, matches, mergeIntoForm, recordMeta, splitRecoveryKey, titleOf, toPayload, valuesFromRecord } from './form';
import type { KindDef, RecordView } from './types';

const kind: KindDef = {
  kind: 'test',
  label: 'Test',
  label_plural: 'Tests',
  title_field: 'name',
  sort_field: 'name',
  sort_desc: false,
  personal: false,
  fields: [
    { name: 'name', label: 'Name', type: 'text', required: true },
    { name: 'notizen', label: 'Notizen', type: 'long_text', required: false },
    { name: 'anzahl', label: 'Anzahl', type: 'number', required: false },
    { name: 'aktiv', label: 'Aktiv', type: 'bool', required: false },
    { name: 'art', label: 'Art', type: 'choice', options: ['a', 'b'], required: false },
    { name: 'status', label: 'Status', type: 'choice', options: ['Offen', 'Erledigt'], required: true, default: 'Offen' },
    { name: 'schule', label: 'Schule', type: 'ref', kind: 'schule', required: false },
    { name: 'kontakte', label: 'Kontakte', type: 'ref_list', kind: 'x', required: false },
  ],
};

const record = (fields: RecordView['fields']): RecordView => ({ id: 'r1', kind: 'test', fields });

describe('Formularwerte', () => {
  it('beginnt leer', () => {
    expect(emptyValues(kind)).toEqual({ name: '', notizen: '', anzahl: '', aktiv: false, art: '', status: 'Offen', schule: '', kontakte: [] });
  });

  it('liest einen Eintrag ins Formular', () => {
    const values = valuesFromRecord(kind, record({ name: 'A', anzahl: 3, aktiv: true, kontakte: ['x', 'y'] }));
    expect(values).toMatchObject({ name: 'A', anzahl: '3', aktiv: true, kontakte: ['x', 'y'], notizen: '' });
  });

  it('ignoriert ungültige Listeneinträge', () => {
    const values = valuesFromRecord(kind, record({ name: 'A', kontakte: ['x', 1 as unknown as string] }));
    expect(values.kontakte).toEqual(['x']);
  });
});

describe('Nutzlast für den Kern', () => {
  it('macht leere Eingaben zu null, damit Felder geleert werden', () => {
    expect(toPayload(kind, emptyValues(kind))).toEqual({
      name: null, notizen: null, anzahl: null, aktiv: false, art: null, status: 'Offen', schule: null, kontakte: null,
    });
  });

  it('schneidet Leerraum ab und wandelt Zahlen um', () => {
    const values = { ...emptyValues(kind), name: '  Anna  ', anzahl: ' 3,5 ', aktiv: true, kontakte: ['a'] };
    expect(toPayload(kind, values)).toMatchObject({ name: 'Anna', anzahl: 3.5, aktiv: true, kontakte: ['a'] });
  });

  it('lehnt ungültige Zahlen ab', () => {
    expect(() => toPayload(kind, { ...emptyValues(kind), anzahl: 'viele' })).toThrow(FormError);
  });

  it('lässt interne Felder weg, weil der Kern sie selbst verwaltet', () => {
    const withInternal: KindDef = {
      ...kind,
      fields: [...kind.fields, { name: 'datei', label: 'Datei', type: 'text', required: false, internal: true }],
    };
    const payload = toPayload(withInternal, { ...emptyValues(withInternal), name: 'Anna', datei: 'x.pdf' });
    expect(payload).not.toHaveProperty('datei');
    expect(payload.name).toBe('Anna');
  });
});

describe('Formular und geänderter Eintrag', () => {
  const initial = { name: 'A', status: 'Entwurf', notizen: '' };

  it('übernimmt Änderungen von außen in Felder, die der Nutzer nicht angefasst hat', () => {
    const next = { name: 'A', status: 'Genehmigt', notizen: '' };
    expect(mergeIntoForm(initial, initial, next)).toEqual(next);
  });

  it('lässt Eingaben des Nutzers stehen', () => {
    const typed = { name: 'A neu', status: 'Entwurf', notizen: 'Text' };
    const next = { name: 'A', status: 'Genehmigt', notizen: '' };
    expect(mergeIntoForm(typed, initial, next)).toEqual({ name: 'A neu', status: 'Genehmigt', notizen: 'Text' });
  });

  it('erkennt geänderte Felder, auch bei Listen', () => {
    const base = { a: 'x', liste: ['1', '2'], an: false };
    expect(changedFields(base, base)).toEqual([]);
    expect(changedFields({ ...base, liste: ['1'], an: true }, base).sort()).toEqual(['an', 'liste']);
  });
});

describe('Vorgabewerte und Zusatzzeile', () => {
  it('belegt Ja/Nein-Felder mit der Vorgabe vor', () => {
    const k: KindDef = { ...kind, fields: [{ name: 'beilegen', label: 'Beilegen', type: 'bool', required: false, default: 'true' }, { name: 'x', label: 'X', type: 'bool', required: false }] };
    expect(emptyValues(k)).toEqual({ beilegen: true, x: false });
  });

  it('zeigt Datum, Status und Art je nach Typ', () => {
    const rec = (k: string, fields: RecordView['fields']): [KindDef, RecordView] => [{ ...kind, kind: k }, { id: 'a', kind: k, fields }];
    expect(recordMeta(...rec('reise', { reise_beginn: '2026-11-16T06:45', status: 'Entwurf' }))).toBe('16.11.2026 · Entwurf');
    expect(recordMeta(...rec('reise', { status: 'Bezahlt' }))).toBe('Bezahlt');
    expect(recordMeta(...rec('beratung', { datum: '2026-10-30' }))).toBe('30.10.2026');
    expect(recordMeta(...rec('protokoll', { datum: '2026-10-30', art: 'Jour fixe' }))).toBe('30.10.2026 · Jour fixe');
    expect(recordMeta(...rec('schule', { name: 'x' }))).toBe('');
  });
});

describe('Anzeige und Suche', () => {
  it('liefert den Titel', () => {
    expect(titleOf(kind, record({ name: 'Anna' }))).toBe('Anna');
    expect(titleOf(kind, record({}))).toBe('(ohne Titel)');
  });

  it('sucht in allen Textfeldern ohne Beachtung der Schreibweise', () => {
    const r = record({ name: 'Grundschule', notizen: 'Medienkonzept' });
    expect(matches(r, '')).toBe(true);
    expect(matches(r, 'GRUND')).toBe(true);
    expect(matches(r, 'konzept')).toBe(true);
    expect(matches(r, 'gymnasium')).toBe(false);
  });

  it('teilt den Recovery-Key in Gruppen', () => {
    expect(splitRecoveryKey('ABCD-EFGH-IJKL')).toEqual(['ABCD', 'EFGH', 'IJKL']);
  });
});
