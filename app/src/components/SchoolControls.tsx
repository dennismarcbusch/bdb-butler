import {
  GROUP_CHOICES,
  NO_FILTER,
  distinctValues,
  isFiltered,
  optionsOf,
  type GroupChoice,
  type SchoolFilter,
} from '../gruppen';
import type { KindDef, RecordView } from '../types';

interface Props {
  kind: KindDef | undefined;
  /** Alle Schulen (ungefiltert): daraus ergeben sich die wählbaren Orte und Schularten. */
  schools: RecordView[];
  group: GroupChoice;
  onGroup: (group: GroupChoice) => void;
  filter: SchoolFilter;
  onFilter: (filter: SchoolFilter) => void;
}

/** Auswahl für Schulen: nach Schulart, Ort oder Trägerschaft gruppieren und darauf eingrenzen. */
export function SchoolControls({ kind, schools, group, onGroup, filter, onFilter }: Props) {
  const set = (patch: Partial<SchoolFilter>) => onFilter({ ...filter, ...patch });
  const traeger = distinctValues(schools, 'traegerschaft', optionsOf(kind, 'traegerschaft'));
  const arten = distinctValues(schools, 'schulart', optionsOf(kind, 'schulart'));
  const orte = distinctValues(schools, 'ort');
  return (
    <div class="school-controls" role="group" aria-label="Schulen gliedern">
      <label>
        Gruppieren nach
        <select value={group} onChange={(e) => onGroup(e.currentTarget.value as GroupChoice)}>
          {GROUP_CHOICES.map((c) => (
            <option key={c.id} value={c.id}>{c.label}</option>
          ))}
        </select>
      </label>
      <label>
        Trägerschaft
        <select value={filter.traegerschaft} onChange={(e) => set({ traegerschaft: e.currentTarget.value })}>
          <option value="">Alle</option>
          {traeger.map((v) => <option key={v} value={v}>{v}</option>)}
        </select>
      </label>
      <label>
        Schulart
        <select value={filter.schulart} onChange={(e) => set({ schulart: e.currentTarget.value })}>
          <option value="">Alle</option>
          {arten.map((v) => <option key={v} value={v}>{v}</option>)}
        </select>
      </label>
      <label>
        Ort
        <select value={filter.ort} onChange={(e) => set({ ort: e.currentTarget.value })}>
          <option value="">Alle</option>
          {orte.map((v) => <option key={v} value={v}>{v}</option>)}
        </select>
      </label>
      {isFiltered(filter) && (
        <button type="button" class="link" onClick={() => onFilter(NO_FILTER)}>Filter zurücksetzen</button>
      )}
    </div>
  );
}
