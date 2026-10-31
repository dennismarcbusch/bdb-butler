import { describe, expect, it } from 'vitest';
import { CHECKLISTS, CHECKLIST_TEMPLATES, neighbours, progress, progressLabel } from './fortbildung';

describe('Fortbildungsprozess', () => {
  it('zählt Aufgaben einer Checkliste', () => {
    expect(progress('- [x] a\n- [ ] b\n- [ ] c')).toEqual({ done: 1, total: 3 });
    expect(progress('')).toEqual({ done: 0, total: 0 });
    expect(progress(undefined)).toEqual({ done: 0, total: 0 });
    expect(progress('Nur Text ohne Aufgaben')).toEqual({ done: 0, total: 0 });
  });

  it('beschreibt den Fortschritt', () => {
    expect(progressLabel({ done: 2, total: 6 })).toBe('2 von 6 Aufgaben erledigt');
    expect(progressLabel({ done: 0, total: 0 })).toBe('keine Aufgaben');
  });

  it('bestimmt die Nachbarphasen', () => {
    const phases = ['Planung', 'Ausschreibung', 'Abgeschlossen'];
    expect(neighbours(phases, 'Planung')).toEqual({ previous: null, next: 'Ausschreibung' });
    expect(neighbours(phases, 'Ausschreibung')).toEqual({ previous: 'Planung', next: 'Abgeschlossen' });
    expect(neighbours(phases, 'Abgeschlossen')).toEqual({ previous: 'Ausschreibung', next: null });
    expect(neighbours(phases, 'Unsinn')).toEqual({ previous: null, next: null });
  });

  it('liefert für jede Checkliste eine Vorlage mit offenen Aufgaben', () => {
    for (const { field } of CHECKLISTS) {
      const p = progress(CHECKLIST_TEMPLATES[field]);
      expect(p.total).toBeGreaterThanOrEqual(5);
      expect(p.done).toBe(0);
    }
    expect(CHECKLIST_TEMPLATES.checkliste_nachbereitung).toContain('Teilnahmebescheinigungen');
    expect(CHECKLIST_TEMPLATES.checkliste_nachbereitung).toContain('Reisekostenabrechnungen versenden');
  });
});
