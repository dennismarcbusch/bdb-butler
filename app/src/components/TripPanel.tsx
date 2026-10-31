import { useCallback, useEffect, useId, useState } from 'preact/hooks';
import { api, fileToBase64, openFile } from '../api';
import { messageOf } from '../errors';
import type { RecordView } from '../types';
import { MailPanel } from './MailPanel';

interface Props {
  trip: RecordView;
  revision: number;
  /// Etwas an der Reise hat sich geändert (Dateien, Status).
  onChanged: () => void;
}

const ATTACHMENT_KINDS = ['Fahrkarte', 'Teilnahmebescheinigung', 'Hotelrechnung', 'Sonstiges'];

function str(trip: RecordView, field: string): string {
  const v = trip.fields[field];
  return typeof v === 'string' ? v : '';
}

function baseName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

/** Dokumente einer Reise: Antrag, Abrechnung, Genehmigung, Anhänge und Mailversand. */
export function TripPanel({ trip, revision, onChanged }: Props) {
  const [attachments, setAttachments] = useState<RecordView[]>([]);
  const [mail, setMail] = useState<'antrag' | 'abrechnung' | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attachmentKind, setAttachmentKind] = useState('Fahrkarte');
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<{ name: string; price: number | null }[] | null>(null);
  const kindId = useId();
  // Häkchen „beilegen“ reagieren sofort, die Speicherung läuft danach. Bei einem Fehler springt das Häkchen zurück.
  const [enclose, setEnclose] = useState<Record<string, boolean>>({});

  const loadAttachments = useCallback(async () => {
    try {
      const all = await api.list('anhang');
      setAttachments(all.filter((a) => a.fields.reise === trip.id));
    } catch {
      setAttachments([]);
    }
  }, [trip.id]);

  useEffect(() => {
    void loadAttachments();
  }, [loadAttachments, revision]);

  async function run(action: () => Promise<unknown>, done: string) {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await action();
      setMessage(done);
      await loadAttachments();
      onChanged();
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  async function upload(file: File | undefined, action: (name: string, data: string) => Promise<unknown>, done: string) {
    if (!file) return;
    await run(async () => action(file.name, await fileToBase64(file)), done);
  }

  const open = async (path: string) => {
    setError(null);
    try {
      await openFile(path);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const antrag = str(trip, 'antrag_pdf');
  const abrechnung = str(trip, 'abrechnung_pdf');
  const formular = str(trip, 'veranstalter_formular');
  const genehmigung = str(trip, 'genehmigung_pdf');

  return (
    <section class="trip" aria-label="Dokumente der Reise">
      <h3>Dokumente</h3>
      {error && <p class="error" role="alert">{error}</p>}
      {message && <p class="ok" role="status">{message}</p>}

      <div class="doc">
        <h4>Dienstreiseantrag</h4>
        <div class="row">
          <button type="button" disabled={busy} onClick={() => void run(() => api.reiseAntrag(trip.id), 'Antrag erzeugt.')}>
            {antrag ? 'Antrag neu erzeugen' : 'Antrag erzeugen'}
          </button>
          {antrag && <button type="button" onClick={() => void open(antrag)}>Antrag öffnen</button>}
          {antrag && <button type="button" onClick={() => setMail('antrag')}>Antrag per Mail senden …</button>}
        </div>
        {antrag && <p class="muted" data-testid="antrag-datei">{baseName(antrag)}{str(trip, 'antrag_versendet_am') ? ` · gesendet ${str(trip, 'antrag_versendet_am').replace('T', ' ')}` : ''}</p>}
        {antrag && <p class="muted">Änderungen an der Reise machen einen erzeugten Antrag veraltet. Dann neu erzeugen.</p>}
      </div>

      <div class="doc">
        <h4>Genehmigung</h4>
        <label class="file">
          Genehmigung hinzufügen (PDF)
          <input type="file" accept="application/pdf,image/*" disabled={busy} onChange={(e) => void upload(e.currentTarget.files?.[0], (n, d) => api.reiseGenehmigung(trip.id, n, d), 'Genehmigung gespeichert.')} />
        </label>
        {genehmigung && (
          <p class="muted">
            {baseName(genehmigung)} <button type="button" class="link small" onClick={() => void open(genehmigung)}>öffnen</button>
          </p>
        )}
      </div>

      <div class="doc">
        <h4>Abrechnung</h4>
        <label class="file">
          Abrechnungsformular des Veranstalters (ausfüllbares PDF)
          <input type="file" accept="application/pdf" disabled={busy} onChange={(e) => void upload(e.currentTarget.files?.[0], (n, d) => api.reiseFormular(trip.id, n, d), 'Formular gespeichert.')} />
        </label>
        {formular && (
          <p class="muted">
            {baseName(formular)} <button type="button" class="link small" onClick={() => void open(formular)}>öffnen</button>
          </p>
        )}
        <div class="row">
          <button
            type="button"
            disabled={busy || !formular}
            title={formular ? undefined : 'Zuerst das Formular des Veranstalters hinzufügen'}
            onClick={() => void run(() => api.reiseAbrechnung(trip.id), 'Abrechnung erzeugt.')}
          >
            {abrechnung ? 'Abrechnung neu erzeugen' : 'Abrechnung erzeugen'}
          </button>
          {abrechnung && <button type="button" onClick={() => void open(abrechnung)}>Abrechnung öffnen</button>}
          {abrechnung && <button type="button" onClick={() => setMail('abrechnung')}>Abrechnung per Mail senden …</button>}
        </div>
        <p class="muted">
          Ohne Formular des Veranstalters: den Erstattungsantrag R002 des LfF mit den Grunddaten vorausfüllen. Tagegeld, Übernachtung,
          Art der BahnCard und weitere Fahrtkosten bitte im PDF ergänzen. Unterschrift und Prüfung bleiben bei dir.
        </p>
        <div class="row">
          <button type="button" disabled={busy} onClick={() => void run(() => api.reiseR002(trip.id), 'Erstattungsantrag R002 erzeugt.')}>
            Erstattungsantrag R002 (LfF) vorausfüllen
          </button>
        </div>
        {abrechnung && <p class="muted" data-testid="abrechnung-datei">{baseName(abrechnung)}{str(trip, 'abrechnung_versendet_am') ? ` · gesendet ${str(trip, 'abrechnung_versendet_am').replace('T', ' ')}` : ''}</p>}
      </div>

      <div class="doc">
        <h4>Anhänge für die Abrechnung</h4>
        {attachments.length === 0 && <p class="muted">Keine Anhänge.</p>}
        <ul class="attachments">
          {attachments.map((a) => (
            <li key={a.id}>
              <label class="check">
                <input
                  type="checkbox"
                  checked={enclose[a.id] ?? a.fields.beilegen !== false}
                  onChange={(e) => {
                    const value = e.currentTarget.checked;
                    setEnclose((prev) => ({ ...prev, [a.id]: value }));
                    void run(() => api.save('anhang', a.id, { beilegen: value }), 'Gespeichert.').then(() =>
                      setEnclose((prev) => {
                        const { [a.id]: _gone, ...rest } = prev;
                        return rest;
                      }),
                    );
                  }}
                />
                <span>{String(a.fields.art ?? '')}: {String(a.fields.name ?? '')}</span>
              </label>
              <button type="button" class="link small" onClick={() => void open(String(a.fields.datei ?? ''))}>öffnen</button>
              {confirmDelete !== a.id && <button type="button" class="link small danger" onClick={() => setConfirmDelete(a.id)}>löschen</button>}
              {confirmDelete === a.id && (
                <>
                  <button type="button" class="small danger" onClick={() => { setConfirmDelete(null); void run(() => api.anhangDelete(a.id), 'Anhang gelöscht.'); }}>Wirklich löschen</button>
                  <button type="button" class="small" onClick={() => setConfirmDelete(null)}>Doch nicht</button>
                </>
              )}
            </li>
          ))}
        </ul>
        <div class="row">
          <button
            type="button"
            disabled={busy || !attachments.some((a) => a.fields.art === 'Fahrkarte')}
            title={attachments.some((a) => a.fields.art === 'Fahrkarte') ? undefined : 'Zuerst eine Fahrkarte als Anhang hinzufügen'}
            onClick={() => {
              setError(null);
              api.reiseTicketpreis(trip.id).then((r) => setSuggestions(r.candidates), (e) => setError(messageOf(e)));
            }}
          >
            Preis aus Fahrkarten erkennen
          </button>
        </div>
        {suggestions && (
          <ul class="suggestions" aria-label="Erkannte Preise">
            {suggestions.map((s) => (
              <li key={s.name}>
                {s.name}:{' '}
                {s.price === null ? (
                  <span class="muted">kein Preis erkannt</span>
                ) : (
                  <>
                    <strong>{s.price.toFixed(2).replace('.', ',')} €</strong>
                    <button type="button" class="small" disabled={busy} onClick={() => void run(() => api.save('reise', trip.id, { fahrkartenpreis: s.price }), 'Fahrkartenpreis übernommen.')}>
                      Als Fahrkartenpreis übernehmen
                    </button>
                  </>
                )}
              </li>
            ))}
            <li class="muted">Vorschlag aus dem Text der PDF-Datei, bitte prüfen. Bei Bildern oder Scans klappt das nicht.</li>
          </ul>
        )}
        <div class="row">
          <div class="inline">
            <label for={kindId}>Art</label>
            <select id={kindId} value={attachmentKind} onChange={(e) => setAttachmentKind(e.currentTarget.value)}>
              {ATTACHMENT_KINDS.map((k) => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
          </div>
          <label class="file">
            Anhang hinzufügen
            <input type="file" disabled={busy} onChange={(e) => void upload(e.currentTarget.files?.[0], (n, d) => api.anhangAdd(trip.id, attachmentKind, n, d), 'Anhang gespeichert.')} />
          </label>
        </div>
      </div>

      {mail && (
        <MailPanel
          tripId={trip.id}
          kind={mail}
          onClose={() => setMail(null)}
          onSent={() => {
            setMail(null);
            setMessage(mail === 'antrag' ? 'Antrag gesendet.' : 'Abrechnung gesendet.');
            onChanged();
          }}
        />
      )}
    </section>
  );
}
