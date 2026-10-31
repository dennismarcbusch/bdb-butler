// Tätigkeitsbericht: wertet die Einträge der Module für einen Zeitraum aus und erzeugt den Berichtstext als
// Markdown. Alles sind reine Funktionen. Der Bericht enthält Zahlen, Titel und Schulnamen, keine Personen.

import { dueState, formatDate } from './auftrag';
import type { RecordView } from './types';

export interface Period {
  /// Erster und letzter Tag, jeweils einschließlich (`JJJJ-MM-TT`).
  from: string;
  to: string;
}

export interface PeriodPreset extends Period {
  label: string;
}

/** Häufig gebrauchte Zeiträume, ausgehend vom heutigen Datum (`JJJJ-MM-TT`). Das Schuljahr läuft vom 1.8. bis 31.7. */
export function periodPresets(today: string): PeriodPreset[] {
  const year = Number(today.slice(0, 4));
  const month = Number(today.slice(5, 7));
  const schoolStart = month >= 8 ? year : year - 1;
  const schoolYear = (start: number): Period => ({ from: `${start}-08-01`, to: `${start + 1}-07-31` });
  return [
    { label: `Kalenderjahr ${year}`, from: `${year}-01-01`, to: `${year}-12-31` },
    { label: `Kalenderjahr ${year - 1}`, from: `${year - 1}-01-01`, to: `${year - 1}-12-31` },
    { label: `Schuljahr ${schoolStart}/${String(schoolStart + 1).slice(2)}`, ...schoolYear(schoolStart) },
    { label: `Schuljahr ${schoolStart - 1}/${String(schoolStart).slice(2)}`, ...schoolYear(schoolStart - 1) },
  ];
}

function text(r: RecordView, field: string): string {
  const v = r.fields[field];
  return typeof v === 'string' ? v : '';
}

function num(r: RecordView, field: string): number {
  const v = r.fields[field];
  return typeof v === 'number' && Number.isFinite(v) ? v : 0;
}

/** Liegt der Tag (die ersten zehn Zeichen von `value`) im Zeitraum? Ungültige oder leere Werte liegen nie darin. */
export function inPeriod(value: string, period: Period): boolean {
  const day = value.slice(0, 10);
  return /^\d{4}-\d{2}-\d{2}$/.test(day) && day >= period.from && day <= period.to;
}

/** `90` als `1 Std. 30 Min.`, `45` als `45 Min.`, `120` als `2 Std.`. */
export function formatMinutes(minutes: number): string {
  const total = Math.max(0, Math.round(minutes));
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m} Min.`;
  return m === 0 ? `${h} Std.` : `${h} Std. ${m} Min.`;
}

export interface Count {
  name: string;
  count: number;
  minutes: number;
}

export interface Report {
  period: Period;
  consultations: { count: number; minutes: number; byKind: Count[]; bySchool: Count[] };
  trainings: { held: RecordView[]; heldParticipants: number; upcoming: RecordView[] };
  trips: { list: RecordView[] };
  protocols: { count: number; byKind: Count[] };
  orders: { done: number; issued: number; open: number; overdue: number };
  requests: { received: number; planned: number; open: number };
}

const HELD_PHASES = ['Durchführung', 'Nachbereitung', 'Abgeschlossen'];

function group(items: { name: string; minutes: number }[]): Count[] {
  const map = new Map<string, Count>();
  for (const item of items) {
    const entry = map.get(item.name) ?? { name: item.name, count: 0, minutes: 0 };
    entry.count += 1;
    entry.minutes += item.minutes;
    map.set(item.name, entry);
  }
  return [...map.values()].sort((a, b) => b.count - a.count || a.name.localeCompare(b.name, 'de'));
}

/** Wertet alle (nicht gelöschten) Einträge für den Zeitraum aus. `today` bestimmt, was überfällig ist. */
export function buildReport(records: RecordView[], period: Period, today: string): Report {
  const of = (kind: string) => records.filter((r) => r.kind === kind);
  const schools = new Map(of('schule').map((s) => [s.id, text(s, 'name')]));

  const consultations = of('beratung').filter((r) => inPeriod(text(r, 'datum'), period));
  const protocols = of('protokoll').filter((r) => inPeriod(text(r, 'datum'), period));
  const trainings = of('fortbildung').filter((r) => inPeriod(text(r, 'beginn'), period));
  const orders = of('auftrag');
  const requests = of('bedarf').filter((r) => inPeriod(text(r, 'eingegangen_am'), period));
  const byStart = (a: RecordView, b: RecordView) => text(a, 'beginn').localeCompare(text(b, 'beginn'));

  const held = trainings.filter((r) => HELD_PHASES.includes(text(r, 'phase'))).sort(byStart);
  return {
    period,
    consultations: {
      count: consultations.length,
      minutes: consultations.reduce((sum, r) => sum + num(r, 'dauer'), 0),
      byKind: group(consultations.map((r) => ({ name: text(r, 'art') || 'ohne Angabe', minutes: num(r, 'dauer') }))),
      bySchool: group(consultations.map((r) => ({ name: schools.get(text(r, 'schule')) || 'ohne Schule', minutes: num(r, 'dauer') }))),
    },
    trainings: {
      held,
      heldParticipants: held.reduce((sum, r) => sum + (num(r, 'teilnehmer_tatsaechlich') || num(r, 'teilnehmer_geplant')), 0),
      upcoming: trainings.filter((r) => !HELD_PHASES.includes(text(r, 'phase'))).sort(byStart),
    },
    trips: {
      list: of('reise')
        .filter((r) => inPeriod(text(r, 'reise_beginn'), period) && text(r, 'status') !== 'Entwurf')
        .sort((a, b) => text(a, 'reise_beginn').localeCompare(text(b, 'reise_beginn'))),
    },
    protocols: {
      count: protocols.length,
      byKind: group(protocols.map((r) => ({ name: text(r, 'art') || 'ohne Angabe', minutes: 0 }))),
    },
    orders: {
      done: orders.filter((r) => text(r, 'status') === 'Erledigt' && inPeriod(text(r, 'erledigt_am'), period)).length,
      issued: orders.filter((r) => inPeriod(text(r, 'erteilt_am'), period)).length,
      open: orders.filter((r) => text(r, 'status') !== 'Erledigt').length,
      overdue: orders.filter((r) => dueState(r, today) === 'overdue').length,
    },
    requests: {
      received: requests.length,
      planned: requests.filter((r) => text(r, 'status') === 'Eingeplant').length,
      open: requests.filter((r) => text(r, 'status') === 'Offen').length,
    },
  };
}

function counts(list: Count[], withMinutes: boolean): string {
  return list.map((c) => `${c.name} ${c.count}${withMinutes && c.minutes > 0 ? ` (${formatMinutes(c.minutes)})` : ''}`).join(', ');
}

/** Berichtstext als Markdown. Leere Abschnitte nennen das ausdrücklich. */
export function reportMarkdown(report: Report, title = 'Tätigkeitsbericht'): string {
  const lines: string[] = [`# ${title}`, '', `Zeitraum: ${formatDate(report.period.from)} bis ${formatDate(report.period.to)}`, ''];
  const { consultations: c, trainings: t, trips, protocols: p, orders: o, requests: r } = report;

  lines.push('## Beratungen', '');
  if (c.count === 0) lines.push('- Keine Beratungen im Zeitraum.');
  else {
    lines.push(`- Anzahl: ${c.count}`);
    if (c.minutes > 0) lines.push(`- Beratungszeit: ${formatMinutes(c.minutes)}`);
    lines.push(`- nach Art: ${counts(c.byKind, true)}`);
    lines.push(`- nach Schule: ${counts(c.bySchool, true)}`);
  }

  lines.push('', '## Fortbildungen', '');
  if (t.held.length === 0 && t.upcoming.length === 0) lines.push('- Keine Fortbildungen im Zeitraum.');
  if (t.held.length > 0) {
    lines.push(`- Durchgeführt: ${t.held.length}${t.heldParticipants > 0 ? ` (Teilnehmende insgesamt: ${t.heldParticipants})` : ''}`);
    for (const f of t.held) {
      const n = num(f, 'teilnehmer_tatsaechlich') || num(f, 'teilnehmer_geplant');
      lines.push(`  - ${formatDate(text(f, 'beginn').slice(0, 10))}: ${text(f, 'titel')}${text(f, 'ort') ? `, ${text(f, 'ort')}` : ''}${n > 0 ? `, ${n} Teilnehmende` : ''}`);
    }
  }
  if (t.upcoming.length > 0) {
    lines.push(`- In Planung oder Ausschreibung: ${t.upcoming.length}`);
    for (const f of t.upcoming) lines.push(`  - ${formatDate(text(f, 'beginn').slice(0, 10))}: ${text(f, 'titel')} (${text(f, 'phase')})`);
  }

  lines.push('', '## Dienstreisen', '');
  if (trips.list.length === 0) lines.push('- Keine Dienstreisen im Zeitraum.');
  else {
    lines.push(`- Anzahl: ${trips.list.length}`);
    for (const trip of trips.list) {
      lines.push(`  - ${formatDate(text(trip, 'reise_beginn').slice(0, 10))}: ${text(trip, 'zweck')}, ${text(trip, 'geschaeftsort')} (${text(trip, 'status')})`);
    }
  }

  lines.push('', '## Protokolle', '');
  lines.push(p.count === 0 ? '- Keine Protokolle im Zeitraum.' : `- Anzahl: ${p.count} (${counts(p.byKind, false)})`);

  lines.push('', '## Aufträge', '');
  lines.push(`- Erteilt im Zeitraum: ${o.issued}`, `- Erledigt im Zeitraum: ${o.done}`, `- Aktuell offen: ${o.open}${o.overdue > 0 ? `, davon überfällig: ${o.overdue}` : ''}`);

  lines.push('', '## Bedarfsanmeldungen', '');
  lines.push(r.received === 0 ? '- Keine Anmeldungen im Zeitraum.' : `- Eingegangen: ${r.received} (eingeplant ${r.planned}, offen ${r.open})`);
  return lines.join('\n') + '\n';
}
