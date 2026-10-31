import { describe, expect, it } from 'vitest';
import { buildReport, formatMinutes, inPeriod, periodPresets, reportMarkdown, type Period } from './bericht';
import type { RecordView } from './types';

const rec = (kind: string, id: string, fields: RecordView['fields']): RecordView => ({ id, kind, fields });
const TODAY = '2026-10-31';
const YEAR: Period = { from: '2026-01-01', to: '2026-12-31' };

describe('Zeiträume', () => {
  it('bietet Kalenderjahre und Schuljahre an, das Schuljahr beginnt am 1. August', () => {
    const inAutumn = periodPresets('2026-10-31');
    expect(inAutumn[0]).toEqual({ label: 'Kalenderjahr 2026', from: '2026-01-01', to: '2026-12-31' });
    expect(inAutumn[1]?.label).toBe('Kalenderjahr 2025');
    expect(inAutumn[2]).toEqual({ label: 'Schuljahr 2026/27', from: '2026-08-01', to: '2027-07-31' });
    expect(inAutumn[3]).toEqual({ label: 'Schuljahr 2025/26', from: '2025-08-01', to: '2026-07-31' });
    // Im Frühjahr gehört der Tag zum Schuljahr, das im Vorjahr begann.
    expect(periodPresets('2027-03-15')[2]).toEqual({ label: 'Schuljahr 2026/27', from: '2026-08-01', to: '2027-07-31' });
    // Grenztage.
    expect(periodPresets('2026-07-31')[2]?.label).toBe('Schuljahr 2025/26');
    expect(periodPresets('2026-08-01')[2]?.label).toBe('Schuljahr 2026/27');
  });

  it('prüft, ob ein Tag im Zeitraum liegt, einschließlich der Grenzen', () => {
    expect(inPeriod('2026-01-01', YEAR)).toBe(true);
    expect(inPeriod('2026-12-31T23:59', YEAR)).toBe(true);
    expect(inPeriod('2025-12-31', YEAR)).toBe(false);
    expect(inPeriod('2027-01-01', YEAR)).toBe(false);
    expect(inPeriod('', YEAR)).toBe(false);
    expect(inPeriod('kaputt', YEAR)).toBe(false);
  });

  it('formatiert Minuten', () => {
    expect(formatMinutes(90)).toBe('1 Std. 30 Min.');
    expect(formatMinutes(45)).toBe('45 Min.');
    expect(formatMinutes(120)).toBe('2 Std.');
    expect(formatMinutes(0)).toBe('0 Min.');
    expect(formatMinutes(-5)).toBe('0 Min.');
  });
});

const data: RecordView[] = [
  rec('schule', 's1', { name: 'Grundschule am See' }),
  rec('schule', 's2', { name: 'Gymnasium Nord' }),
  rec('beratung', 'b1', { thema: 'A', datum: '2026-03-01', art: 'Vor Ort', dauer: 90, schule: 's1' }),
  rec('beratung', 'b2', { thema: 'B', datum: '2026-04-01', art: 'Online', dauer: 30, schule: 's1' }),
  rec('beratung', 'b3', { thema: 'C', datum: '2026-05-01', art: 'Vor Ort', schule: 's2' }),
  rec('beratung', 'b4', { thema: 'D', datum: '2025-12-31', art: 'Vor Ort', dauer: 600, schule: 's2' }),
  rec('protokoll', 'p1', { titel: 'P1', art: 'Jour fixe', datum: '2026-02-01' }),
  rec('protokoll', 'p2', { titel: 'P2', art: 'Jour fixe', datum: '2026-02-08' }),
  rec('protokoll', 'p3', { titel: 'P3', art: 'Beratung', datum: '2025-01-01' }),
  rec('fortbildung', 'f1', { titel: 'KI-Tag', phase: 'Abgeschlossen', beginn: '2026-06-10T09:00', ort: 'Ulm', teilnehmer_geplant: 30, teilnehmer_tatsaechlich: 24 }),
  rec('fortbildung', 'f2', { titel: 'Tablets', phase: 'Nachbereitung', beginn: '2026-02-10T09:00', teilnehmer_geplant: 20 }),
  rec('fortbildung', 'f3', { titel: 'Datenschutz', phase: 'Ausschreibung', beginn: '2026-11-20T09:00' }),
  rec('fortbildung', 'f4', { titel: 'Anderes Jahr', phase: 'Abgeschlossen', beginn: '2025-06-10T09:00', teilnehmer_tatsaechlich: 99 }),
  rec('reise', 'r1', { zweck: 'Beratung Medienkonzept', geschaeftsort: 'Dillingen', reise_beginn: '2026-11-16T06:45', status: 'Bezahlt' }),
  rec('reise', 'r2', { zweck: 'Entwurf', geschaeftsort: 'Ulm', reise_beginn: '2026-11-20T06:45', status: 'Entwurf' }),
  rec('reise', 'r3', { zweck: 'Früher', geschaeftsort: 'Bonn', reise_beginn: '2026-01-05T06:45', status: 'Abgerechnet' }),
  rec('auftrag', 'a1', { titel: 'x', status: 'Erledigt', erledigt_am: '2026-05-05', erteilt_am: '2026-05-01' }),
  rec('auftrag', 'a2', { titel: 'y', status: 'Offen', erteilt_am: '2026-09-01', frist: '2026-10-01' }),
  rec('auftrag', 'a3', { titel: 'z', status: 'In Arbeit', frist: '2026-12-01' }),
  rec('auftrag', 'a4', { titel: 'alt', status: 'Erledigt', erledigt_am: '2025-05-05' }),
  rec('bedarf', 'd1', { thema: 'KI', status: 'Eingeplant', eingegangen_am: '2026-01-10' }),
  rec('bedarf', 'd2', { thema: 'Tablets', status: 'Offen', eingegangen_am: '2026-02-10' }),
  rec('bedarf', 'd3', { thema: 'Alt', status: 'Offen', eingegangen_am: '2025-02-10' }),
];

describe('Auswertung', () => {
  const report = buildReport(data, YEAR, TODAY);

  it('zählt Beratungen mit Zeit, Art und Schule und lässt andere Zeiträume weg', () => {
    expect(report.consultations.count).toBe(3);
    expect(report.consultations.minutes).toBe(120);
    expect(report.consultations.byKind).toEqual([
      { name: 'Vor Ort', count: 2, minutes: 90 },
      { name: 'Online', count: 1, minutes: 30 },
    ]);
    expect(report.consultations.bySchool).toEqual([
      { name: 'Grundschule am See', count: 2, minutes: 120 },
      { name: 'Gymnasium Nord', count: 1, minutes: 0 },
    ]);
  });

  it('trennt durchgeführte und geplante Fortbildungen und summiert Teilnehmende', () => {
    // Nach Beginn sortiert.
    expect(report.trainings.held.map((f) => f.id)).toEqual(['f2', 'f1']);
    // f1: tatsächlich 24, f2: nur geplant 20.
    expect(report.trainings.heldParticipants).toBe(44);
    expect(report.trainings.upcoming.map((f) => f.id)).toEqual(['f3']);
  });

  it('listet Reisen ohne Entwürfe und ohne andere Zeiträume, nach Datum sortiert', () => {
    expect(report.trips.list.map((r) => r.id)).toEqual(['r3', 'r1']);
  });

  it('zählt Protokolle, Aufträge und Bedarfe', () => {
    expect(report.protocols).toEqual({ count: 2, byKind: [{ name: 'Jour fixe', count: 2, minutes: 0 }] });
    expect(report.orders).toEqual({ done: 1, issued: 2, open: 2, overdue: 1 });
    expect(report.requests).toEqual({ received: 2, planned: 1, open: 1 });
  });

  it('kommt mit leeren Daten und anderen Zeiträumen zurecht', () => {
    const empty = buildReport([], YEAR, TODAY);
    expect(empty.consultations.count).toBe(0);
    expect(empty.trainings.held).toEqual([]);
    const other = buildReport(data, { from: '2030-01-01', to: '2030-12-31' }, TODAY);
    expect(other.consultations.count + other.protocols.count + other.trips.list.length + other.requests.received).toBe(0);
  });
});

describe('Berichtstext', () => {
  it('enthält alle Abschnitte mit den Zahlen', () => {
    const md = reportMarkdown(buildReport(data, YEAR, TODAY));
    expect(md).toContain('# Tätigkeitsbericht');
    expect(md).toContain('Zeitraum: 01.01.2026 bis 31.12.2026');
    expect(md).toContain('- Anzahl: 3');
    expect(md).toContain('- Beratungszeit: 2 Std.');
    expect(md).toContain('- nach Art: Vor Ort 2 (1 Std. 30 Min.), Online 1 (30 Min.)');
    expect(md).toContain('- nach Schule: Grundschule am See 2 (2 Std.), Gymnasium Nord 1');
    expect(md).toContain('- Durchgeführt: 2 (Teilnehmende insgesamt: 44)');
    expect(md).toContain('  - 10.06.2026: KI-Tag, Ulm, 24 Teilnehmende');
    expect(md).toContain('  - 20.11.2026: Datenschutz (Ausschreibung)');
    expect(md).toContain('  - 16.11.2026: Beratung Medienkonzept, Dillingen (Bezahlt)');
    expect(md).not.toContain('Entwurf, Ulm');
    expect(md).toContain('- Anzahl: 2 (Jour fixe 2)');
    expect(md).toContain('- Aktuell offen: 2, davon überfällig: 1');
    expect(md).toContain('- Eingegangen: 2 (eingeplant 1, offen 1)');
  });

  it('sagt bei leeren Abschnitten ausdrücklich, dass es nichts gab', () => {
    const md = reportMarkdown(buildReport([], YEAR, TODAY), 'Bericht 2026');
    expect(md).toContain('# Bericht 2026');
    for (const none of ['Keine Beratungen', 'Keine Fortbildungen', 'Keine Dienstreisen', 'Keine Protokolle', 'Keine Anmeldungen']) {
      expect(md).toContain(none);
    }
    expect(md).toContain('- Erteilt im Zeitraum: 0');
  });

  it('enthält keine personenbezogenen Felder', () => {
    const withPerson = [...data, rec('ansprechpartner', 'x', { name: 'Frau Geheim', telefon: '0123' }), rec('beratene_person', 'y', { name: 'Herr Vertraulich' })];
    const md = reportMarkdown(buildReport(withPerson, YEAR, TODAY));
    expect(md).not.toContain('Geheim');
    expect(md).not.toContain('Vertraulich');
  });
});
