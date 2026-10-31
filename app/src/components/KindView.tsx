import { useCallback, useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import { matches, titleOf } from '../form';
import type { KindDef, RecordView } from '../types';
import { RecordForm } from './RecordForm';
import { RelatedList } from './RelatedList';

interface Props {
  kind: KindDef;
  kinds: KindDef[];
  revision: number;
  /// Eintrag, der beim Öffnen ausgewählt sein soll (zum Beispiel aus dem Graphen).
  initialSelected?: string | null;
  onChanged: () => void;
  onNavigate: (kind: string, id: string) => void;
}

/** Liste mit Suche und Formular für einen Typ. */
export function KindView({ kind, kinds, revision, initialSelected = null, onChanged, onNavigate }: Props) {
  const [records, setRecords] = useState<RecordView[]>([]);
  const [selected, setSelected] = useState<string | 'new' | null>(initialSelected);
  const [query, setQuery] = useState('');
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRecords(await api.list(kind.kind));
      setError(null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [kind.kind]);

  useEffect(() => {
    void load();
  }, [load, revision]);

  const visible = records.filter((r) => matches(r, query));
  const current = selected && selected !== 'new' ? (records.find((r) => r.id === selected) ?? null) : null;

  return (
    <div class="split">
      <section class="list" aria-label={kind.label_plural}>
        <div class="list-head">
          <h2>{kind.label_plural}</h2>
          <button type="button" class="primary" onClick={() => setSelected('new')}>{kind.label} anlegen</button>
        </div>
        <input type="search" placeholder="Suchen …" aria-label="Suchen" value={query} onInput={(e) => setQuery(e.currentTarget.value)} />
        {error && <p class="error" role="alert">{error}</p>}
        <ul>
          {visible.map((r) => (
            <li key={r.id}>
              <button type="button" class={selected === r.id ? 'active' : ''} onClick={() => setSelected(r.id)}>
                {titleOf(kind, r)}
              </button>
            </li>
          ))}
        </ul>
        {visible.length === 0 && <p class="muted">{records.length === 0 ? 'Noch nichts angelegt.' : 'Keine Treffer.'}</p>}
      </section>

      <section class="detail" aria-label="Details">
        {selected === null && <p class="muted">Wähle einen Eintrag oder lege einen neuen an.</p>}
        {selected !== null && (selected === 'new' || current) && (
          <RecordForm
            key={selected}
            kind={kind}
            kinds={kinds}
            record={current}
            onSaved={(id) => {
              setSelected(id);
              void load();
              onChanged();
            }}
            onDeleted={() => {
              setSelected(null);
              void load();
              onChanged();
            }}
            onCancel={() => setSelected(null)}
            onNavigate={onNavigate}
          />
        )}
        {current && <RelatedList kinds={kinds} recordId={current.id} revision={revision} onOpen={onNavigate} />}
      </section>
    </div>
  );
}
