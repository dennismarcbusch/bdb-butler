import { useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import type { MailDraft } from '../types';

interface Props {
  tripId: string;
  kind: 'antrag' | 'abrechnung';
  onClose: () => void;
  /// Die Mail wurde gesendet (Status der Reise hat sich geändert).
  onSent: () => void;
}

function megabytes(bytes: number): string {
  return (bytes / 1024 / 1024).toFixed(2).replace('.', ',');
}

/** Vorschau und Versand der Mail an die Verwaltung (Antrag) oder das LfF (Abrechnung). */
export function MailPanel({ tripId, kind, onClose, onSent }: Props) {
  const [draft, setDraft] = useState<MailDraft | null>(null);
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.mailDraft(tripId, kind).then(
      (d) => {
        setDraft(d);
        setSubject(d.subject);
        setBody(d.body);
        setSelected(d.attachments.filter((a) => a.selected).map((a) => a.key));
      },
      (e) => setError(messageOf(e)),
    );
  }, [tripId, kind]);

  if (!draft) {
    return (
      <section class="mail" aria-label="Mail">
        {error ? <p class="error" role="alert">{error}</p> : <p>Lade …</p>}
        <button type="button" onClick={onClose}>Schließen</button>
      </section>
    );
  }

  const chosen = draft.attachments.filter((a) => a.required || selected.includes(a.key));
  const total = chosen.reduce((sum, a) => sum + a.size, 0);
  // Die Grenze gilt für die Mail: Anhänge werden dort als Base64 etwa ein Drittel größer (Werte vom Kern).
  const encoded = chosen.reduce((sum, a) => sum + a.encoded_size, 0);
  const tooLarge = encoded > draft.max_bytes;
  const title = kind === 'antrag' ? 'Antrag an die Verwaltung senden' : 'Abrechnung an das LfF senden';

  async function send() {
    setBusy(true);
    setError(null);
    try {
      await api.mailSend(tripId, kind, subject, body, selected);
      onSent();
    } catch (e) {
      setError(messageOf(e));
      setBusy(false);
    }
  }

  return (
    <section class="mail" aria-label="Mail">
      <h3>{title}</h3>
      {draft.already_sent && (
        <p class="warn" role="status">
          Dieses Dokument wurde schon am {draft.already_sent.slice(8, 10)}.{draft.already_sent.slice(5, 7)}.{draft.already_sent.slice(0, 4)} um{' '}
          {draft.already_sent.slice(11, 16)} Uhr gesendet.
        </p>
      )}
      <dl class="mail-header">
        <dt>An</dt>
        <dd data-testid="mail-to">{draft.to || <span class="error">fehlt in den Stammdaten</span>}</dd>
        <dt>Von</dt>
        <dd data-testid="mail-from">{draft.from || <span class="error">fehlt in den Stammdaten</span>}</dd>
      </dl>
      <label>
        Betreff
        <input type="text" value={subject} onInput={(e) => setSubject(e.currentTarget.value)} />
      </label>
      <label>
        Text
        <textarea rows={9} value={body} onInput={(e) => setBody(e.currentTarget.value)} />
      </label>
      <fieldset>
        <legend>Anhänge</legend>
        {draft.attachments.map((a) => (
          <label class="check" key={a.key}>
            <input
              type="checkbox"
              checked={a.required || selected.includes(a.key)}
              disabled={a.required}
              onChange={(e) => setSelected(e.currentTarget.checked ? [...selected, a.key] : selected.filter((k) => k !== a.key))}
            />
            {a.name} <span class="muted">({megabytes(a.size)} MB{a.required ? ', immer dabei' : ''})</span>
          </label>
        ))}
        <p class={tooLarge ? 'error' : 'muted'} role="status">
          Zusammen {megabytes(total)} MB, in der Mail {megabytes(encoded)} MB
          {tooLarge ? ` (erlaubt sind ${megabytes(draft.max_bytes)} MB). Bitte Anhänge abwählen.` : '.'}
        </p>
      </fieldset>
      {error && <p class="error" role="alert">{error}</p>}
      <div class="row">
        <button type="button" class="primary" disabled={busy || tooLarge || !draft.to || !draft.from} onClick={() => void send()}>
          {busy ? 'Sende …' : 'Senden'}
        </button>
        <button type="button" onClick={onClose} disabled={busy}>Abbrechen</button>
      </div>
    </section>
  );
}
