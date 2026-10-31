import { useEffect, useMemo, useState } from 'preact/hooks';
import { api, openFile } from '../api';
import { todayIso, formatDate } from '../auftrag';
import { buildReport, periodPresets, reportMarkdown } from '../bericht';
import { messageOf } from '../errors';
import type { RecordView } from '../types';
import { Markdown } from './Markdown';

/** Tätigkeitsbericht für einen Zeitraum aus den Daten aller Module. */
export function ReportView({ revision }: { revision: number }) {
  const today = todayIso();
  const presets = useMemo(() => periodPresets(today), [today]);
  const [from, setFrom] = useState(presets[0]!.from);
  const [to, setTo] = useState(presets[0]!.to);
  const [records, setRecords] = useState<RecordView[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.listAll().then(
      (list) => {
        setRecords(list);
        setError(null);
      },
      (e) => setError(messageOf(e)),
    );
  }, [revision]);

  const valid = /^\d{4}-\d{2}-\d{2}$/.test(from) && /^\d{4}-\d{2}-\d{2}$/.test(to) && from <= to;
  const markdown = useMemo(() => (valid ? reportMarkdown(buildReport(records, { from, to }, today)) : ''), [records, from, to, today, valid]);

  async function save() {
    setError(null);
    setMessage(null);
    try {
      const { path } = await api.reportSave(`Tätigkeitsbericht ${formatDate(from)} bis ${formatDate(to)}`, markdown);
      setSavedPath(path);
      setMessage(`Gespeichert: ${path}`);
    } catch (e) {
      setError(messageOf(e));
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(markdown);
      setMessage('In die Zwischenablage kopiert.');
    } catch {
      setError('Das Kopieren ist nicht möglich.');
    }
  }

  return (
    <section class="report" aria-label="Tätigkeitsbericht">
      <div class="report-controls no-print">
        <h2>Tätigkeitsbericht</h2>
        <div class="row">
          {presets.map((p) => (
            <button key={p.label} type="button" onClick={() => { setFrom(p.from); setTo(p.to); setMessage(null); }}>
              {p.label}
            </button>
          ))}
        </div>
        <div class="row">
          <div class="field">
            <label for="report-from">Von</label>
            <input id="report-from" type="date" value={from} onInput={(e) => setFrom(e.currentTarget.value)} />
          </div>
          <div class="field">
            <label for="report-to">Bis</label>
            <input id="report-to" type="date" value={to} onInput={(e) => setTo(e.currentTarget.value)} />
          </div>
        </div>
        {!valid && <p class="error" role="alert">Bitte einen gültigen Zeitraum wählen (Von nicht nach Bis).</p>}
        {error && <p class="error" role="alert">{error}</p>}
        {message && (
          <p class="ok" role="status">
            {message}
            {savedPath && <button type="button" class="link small" onClick={() => void openFile(savedPath)}>öffnen</button>}
          </p>
        )}
        <div class="row">
          <button type="button" class="primary" disabled={!valid} onClick={() => void save()}>Als Datei speichern</button>
          <button type="button" disabled={!valid} onClick={() => void copy()}>Kopieren</button>
          <button type="button" disabled={!valid} onClick={() => window.print()}>Drucken</button>
        </div>
        <p class="muted">Der Bericht enthält Zahlen, Titel und Schulnamen, keine Personen. Er wird als Markdown-Datei im Ordner „Berichte“ abgelegt.</p>
      </div>
      {valid && (
        <div class="report-body" data-testid="report">
          <Markdown source={markdown} resolve={() => null} onOpen={() => undefined} />
        </div>
      )}
    </section>
  );
}
