import { useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import type { JsonValue, KindDef, RecordView } from '../types';

interface Props {
  kinds: KindDef[];
  conflicts: RecordView[];
  onChanged: () => void;
}

function show(value: JsonValue | undefined): string {
  if (value === undefined || value === null) return '(leer)';
  return typeof value === 'string' ? value : JSON.stringify(value);
}

export function Conflicts({ kinds, conflicts, onChanged }: Props) {
  const [error, setError] = useState<string | null>(null);

  async function keepLocal(conflict: RecordView) {
    try {
      await api.delete(conflict.id);
      onChanged();
    } catch (e) {
      setError(messageOf(e));
    }
  }

  async function takeRemote(conflict: RecordView) {
    const entry = conflict.fields.eintrag;
    const kind = conflict.fields.eintragstyp;
    const field = conflict.fields.feld;
    if (typeof entry !== 'string' || typeof kind !== 'string' || typeof field !== 'string') return;
    try {
      await api.save(kind, entry, { [field]: conflict.fields.fremd ?? null });
      await api.delete(conflict.id);
      onChanged();
    } catch (e) {
      setError(messageOf(e));
    }
  }

  return (
    <section>
      <h2>Konflikte</h2>
      <p class="muted">
        Dasselbe Feld wurde auf zwei Rechnern verschieden geändert. Der jüngere Wert gilt bereits. Hier kannst du den anderen übernehmen oder
        den Konflikt abhaken.
      </p>
      {error && <p class="error" role="alert">{error}</p>}
      {conflicts.length === 0 && <p class="muted">Keine offenen Konflikte.</p>}
      <ul class="conflicts">
        {conflicts.map((c) => {
          const kindLabel = kinds.find((k) => k.kind === c.fields.eintragstyp)?.label ?? String(c.fields.eintragstyp ?? '');
          return (
            <li key={c.id}>
              <strong>{kindLabel}, Feld „{show(c.fields.feld)}“</strong>
              <dl>
                <dt>Gilt jetzt (lokal)</dt>
                <dd>{show(c.fields.lokal)}</dd>
                <dt>Anderer Wert (von einem anderen Rechner)</dt>
                <dd>{show(c.fields.fremd)}</dd>
              </dl>
              <div class="row">
                <button type="button" onClick={() => void keepLocal(c)}>Aktuellen Wert behalten</button>
                <button type="button" onClick={() => void takeRemote(c)}>Anderen Wert übernehmen</button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
