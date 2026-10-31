import { describe, expect, it } from 'vitest';
import { emptyValues, FormError, matches, splitRecoveryKey, titleOf, toPayload, valuesFromRecord } from './form';
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
