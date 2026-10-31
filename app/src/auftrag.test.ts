import { describe, expect, it } from 'vitest';
import { daysBetween, dueLabel, dueState, filterOrders, formatDate, isDone, overdueCount, todayIso } from './auftrag';
import type { RecordView } from './types';

const order = (fields: RecordView['fields']): RecordView => ({ id: 'a', kind: 'auftrag', fields: { status: 'Offen', ...fields } });
const TODAY = '2026-10-31';

describe('Datum', () => {
  it('rechnet Tage, auch über Monats-, Jahres- und Schaltjahrgrenzen', () => {
    expect(daysBetween('2026-10-31', '2026-11-01')).toBe(1);
    expect(daysBetween('2026-10-31', '2026-10-31')).toBe(0);
    expect(daysBetween('2026-10-31', '2026-10-24')).toBe(-7);
    expect(daysBetween('2026-12-31', '2027-01-01')).toBe(1);
    expect(daysBetween('2024-02-28', '2024-03-01')).toBe(2);
    expect(daysBetween('2025-02-28', '2025-03-01')).toBe(1);
  });

  it('liefert null bei ungültigen Daten', () => {
    for (const bad of ['', '31.10.2026', '2026-02-30', '2026-13-01', 'heute']) {
      expect(daysBetween(bad, TODAY)).toBeNull();
      expect(daysBetween(TODAY, bad)).toBeNull();
    }
  });

  it('formatiert deutsch', () => {
    expect(formatDate('2026-10-05')).toBe('05.10.2026');
    expect(formatDate('kein datum')).toBe('kein datum');
  });

  it('liefert das lokale Datum mit führenden Nullen', () => {
    expect(todayIso(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
    expect(todayIso(new Date(2026, 11, 31, 0, 0))).toBe('2026-12-31');
  });
});

describe('Fälligkeit', () => {
  it.each([
    [{ frist: '2026-10-30' }, 'overdue'],
    [{ frist: '2026-10-31' }, 'today'],
    [{ frist: '2026-11-01' }, 'soon'],
    [{ frist: '2026-11-07' }, 'soon'],
    [{ frist: '2026-11-08' }, 'later'],
    [{}, 'none'],
    [{ frist: 'kaputt' }, 'none'],
    [{ frist: '2020-01-01', status: 'Erledigt' }, 'done'],
  ] as const)('%j ist %s', (fields, expected) => {
    expect(dueState(order({ ...fields }), TODAY)).toBe(expected);
  });

  it('beschreibt die Frist in Worten', () => {
    expect(dueLabel(order({ frist: '2026-10-28' }), TODAY)).toBe('überfällig seit 3 Tagen');
    expect(dueLabel(order({ frist: '2026-10-30' }), TODAY)).toBe('überfällig seit 1 Tag');
    expect(dueLabel(order({ frist: '2026-10-31' }), TODAY)).toBe('heute fällig');
    expect(dueLabel(order({ frist: '2026-11-01' }), TODAY)).toBe('morgen fällig');
    expect(dueLabel(order({ frist: '2026-11-04' }), TODAY)).toBe('fällig in 4 Tagen');
    expect(dueLabel(order({ frist: '2026-12-24' }), TODAY)).toBe('Frist 24.12.2026');
    expect(dueLabel(order({}), TODAY)).toBe('ohne Frist');
    expect(dueLabel(order({ status: 'Erledigt', erledigt_am: '2026-10-20' }), TODAY)).toBe('erledigt am 20.10.2026');
    expect(dueLabel(order({ status: 'Erledigt' }), TODAY)).toBe('erledigt');
  });
});

describe('Filter und Zähler', () => {
  const records = [
    order({ frist: '2026-10-01' }),
    order({ status: 'In Arbeit', frist: '2026-10-30' }),
    order({ status: 'Wartet', frist: '2026-12-01' }),
    order({ status: 'Erledigt', frist: '2026-01-01' }),
  ];

  it('filtert nach Offen, Erledigt und Alle', () => {
    expect(filterOrders(records, 'open')).toHaveLength(3);
    expect(filterOrders(records, 'done')).toHaveLength(1);
    expect(filterOrders(records, 'all')).toHaveLength(4);
    expect(filterOrders(records, 'done').every(isDone)).toBe(true);
  });

  it('zählt nur offene überfällige Aufträge', () => {
    expect(overdueCount(records, TODAY)).toBe(2);
    expect(overdueCount([], TODAY)).toBe(0);
  });
});
