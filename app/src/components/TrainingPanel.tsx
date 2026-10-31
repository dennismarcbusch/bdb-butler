import { useCallback, useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import { CHECKLISTS, CHECKLIST_TEMPLATES, neighbours, progress, progressLabel } from '../fortbildung';
import type { KindDef, RecordView } from '../types';

interface Props {
  training: RecordView;
  kinds: KindDef[];
  revision: number;
  /// Etwas an der Fortbildung oder an Bedarfsanmeldungen hat sich geändert.
  onChanged: () => void;
}

function str(r: RecordView, field: string): string {
  const v = r.fields[field];
  return typeof v === 'string' ? v : '';
}

/** Prozess einer Fortbildung: Phasen, Checklisten und zugeordnete Bedarfsanmeldungen. */
export function TrainingPanel({ training, kinds, revision, onChanged }: Props) {
  const def = kinds.find((k) => k.kind === 'fortbildung');
  const phaseField = def?.fields.find((f) => f.name === 'phase');
  const phases = phaseField && phaseField.type === 'choice' ? phaseField.options : [];
  const phase = str(training, 'phase');
  const { previous, next } = neighbours(phases, phase);

  const [requests, setRequests] = useState<RecordView[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadRequests = useCallback(async () => {
    try {
      setRequests(await api.list('bedarf'));
    } catch {
      setRequests([]);
    }
  }, []);

  useEffect(() => {
    void loadRequests();
  }, [loadRequests, revision]);

  async function run(action: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await action();
      await loadRequests();
      onChanged();
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  const assigned = requests.filter((r) => r.fields.fortbildung === training.id);
  const open = requests.filter((r) => r.fields.status === 'Offen' && !r.fields.fortbildung);
  const interested = assigned.reduce((sum, r) => sum + (typeof r.fields.interessenten === 'number' ? r.fields.interessenten : 0), 0);
  const planned = typeof training.fields.teilnehmer_geplant === 'number' ? training.fields.teilnehmer_geplant : null;

  return (
    <section class="training" aria-label="Prozess der Fortbildung">
      <h3>Prozess</h3>
      <ol class="phases" aria-label="Phasen">
        {phases.map((p) => (
          <li key={p} class={p === phase ? 'current' : phases.indexOf(p) < phases.indexOf(phase) ? 'past' : ''} aria-current={p === phase ? 'step' : undefined}>
            {p}
          </li>
        ))}
      </ol>
      {error && <p class="error" role="alert">{error}</p>}
      <div class="row">
        <button type="button" disabled={busy || !previous} onClick={() => previous && void run(() => api.save('fortbildung', training.id, { phase: previous }))}>
          {previous ? `Zurück zu „${previous}“` : 'Zurück'}
        </button>
        <button type="button" class="primary" disabled={busy || !next} onClick={() => next && void run(() => api.save('fortbildung', training.id, { phase: next }))}>
          {next ? `Weiter zu „${next}“` : 'Abgeschlossen'}
        </button>
      </div>

      <h4>Checklisten</h4>
      <ul class="checklists">
        {CHECKLISTS.map(({ field, label }) => {
          const text = str(training, field);
          return (
            <li key={field}>
              <strong>{label}:</strong> <span data-testid={`fortschritt-${field}`}>{progressLabel(progress(text))}</span>
              {text.trim() === '' && (
                <button type="button" class="small" disabled={busy} onClick={() => void run(() => api.save('fortbildung', training.id, { [field]: CHECKLIST_TEMPLATES[field] }))}>
                  Standard-Checkliste einfügen
                </button>
              )}
            </li>
          );
        })}
      </ul>

      <h4>Bedarfsanmeldungen</h4>
      <p class="muted" data-testid="bedarf-summe">
        {assigned.length === 0 ? 'Noch keine Anmeldungen zugeordnet.' : `${assigned.length} zugeordnet, ${interested} Interessenten`}
        {planned !== null ? ` (geplant: ${planned} Teilnehmer)` : ''}
      </p>
      <ul class="requests">
        {assigned.map((r) => (
          <li key={r.id}>
            {str(r, 'thema')}
            {typeof r.fields.interessenten === 'number' ? <span class="muted"> ({r.fields.interessenten} Interessenten)</span> : null}
            <button
              type="button"
              class="link small"
              disabled={busy}
              onClick={() => void run(() => api.save('bedarf', r.id, { fortbildung: null, status: 'Offen' }))}
            >
              lösen
            </button>
          </li>
        ))}
      </ul>
      {open.length > 0 && (
        <fieldset>
          <legend>Offene Anmeldungen zuordnen</legend>
          {open.map((r) => (
            <label class="check" key={r.id}>
              <input type="checkbox" checked={picked.includes(r.id)} onChange={(e) => setPicked(e.currentTarget.checked ? [...picked, r.id] : picked.filter((x) => x !== r.id))} />
              {str(r, 'thema')}
            </label>
          ))}
          <div class="row">
            <button
              type="button"
              disabled={busy || picked.length === 0}
              onClick={() =>
                void run(async () => {
                  for (const id of picked) await api.save('bedarf', id, { fortbildung: training.id, status: 'Eingeplant' });
                  setPicked([]);
                })
              }
            >
              Zuordnen
            </button>
          </div>
        </fieldset>
      )}
    </section>
  );
}
