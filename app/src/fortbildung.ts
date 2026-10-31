// Hilfen für den Prozess einer Fortbildung. Die Phasen selbst und ihre Voraussetzungen stehen im Kern
// (`core/src/fortbildung.rs`), die Oberfläche bekommt die Phasen aus der Typdefinition.

import { extractTasks } from './markdown';

export const CHECKLISTS = [
  { field: 'checkliste_ausschreibung', label: 'Ausschreibung' },
  { field: 'checkliste_durchfuehrung', label: 'Durchführung' },
  { field: 'checkliste_nachbereitung', label: 'Nachbereitung' },
] as const;

export type ChecklistField = (typeof CHECKLISTS)[number]['field'];

/** Standard-Checklisten als Vorschlag. Sie lassen sich beliebig ändern. */
export const CHECKLIST_TEMPLATES: Record<ChecklistField, string> = {
  checkliste_ausschreibung: [
    '- [ ] Ausschreibungstext erstellen',
    '- [ ] Veranstaltung in FIBS anlegen und ausschreiben',
    '- [ ] Veranstaltungsnummer eintragen',
    '- [ ] Referent bestätigt Termin und Honorar',
    '- [ ] Raum und Technik reservieren',
  ].join('\n'),
  checkliste_durchfuehrung: [
    '- [ ] Teilnehmerliste prüfen',
    '- [ ] Raum und Technik prüfen',
    '- [ ] Unterlagen bereitstellen',
    '- [ ] Referent empfangen und Ablauf besprechen',
    '- [ ] Anwesenheit dokumentieren',
    '- [ ] Feedbackbögen bereithalten',
  ].join('\n'),
  checkliste_nachbereitung: [
    '- [ ] Teilnahmebescheinigungen erstellen und versenden',
    '- [ ] Referentenabrechnung einreichen',
    '- [ ] Reisekostenabrechnungen versenden',
    '- [ ] Evaluation auswerten',
    '- [ ] Materialien den Teilnehmenden zur Verfügung stellen',
    '- [ ] Fortbildung in FIBS abschließen',
  ].join('\n'),
};

export interface Progress {
  done: number;
  total: number;
}

/** Erledigte und alle Aufgaben einer Checkliste. */
export function progress(markdown: string | undefined | null): Progress {
  const tasks = extractTasks(markdown ?? '');
  return { done: tasks.filter((t) => t.done).length, total: tasks.length };
}

export function progressLabel(p: Progress): string {
  if (p.total === 0) return 'keine Aufgaben';
  return `${p.done} von ${p.total} Aufgaben erledigt`;
}

/** Vorherige und nächste Phase (`null` am Anfang beziehungsweise Ende). */
export function neighbours(phases: string[], current: string): { previous: string | null; next: string | null } {
  const i = phases.indexOf(current);
  if (i < 0) return { previous: null, next: null };
  return { previous: phases[i - 1] ?? null, next: phases[i + 1] ?? null };
}
