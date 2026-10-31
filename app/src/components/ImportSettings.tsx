import { useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import type { ImportReport } from '../types';

/** Einmaliger Import der Daten des alten Dienstreisebusters (Django). */
export function ImportSettings() {
  const [db, setDb] = useState('');
  const [media, setMedia] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);

  async function run(event: Event) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      setReport(await api.importDjango(db, media));
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form class="import" onSubmit={run} aria-label="Import aus dem alten Dienstreisebuster">
      <h3>Import aus dem alten Dienstreisebuster</h3>
      <p class="muted">
        Übernimmt Stammdaten, Reisen, Dateien und Anhänge aus der Datenbank des alten Tools. Es ändert nichts am alten Tool. Ein erneuter
        Import überspringt, was schon da ist, und überschreibt nichts. Der Ordner für PDFs und Anhänge muss oben eingetragen sein.
      </p>
      <div class="field">
        <label for="import-db">Datenbank des alten Tools (<code>db.sqlite3</code>)</label>
        <input id="import-db" type="text" value={db} placeholder="/…/data/db.sqlite3" onInput={(e) => setDb(e.currentTarget.value)} />
      </div>
      <div class="field">
        <label for="import-media">Ordner mit den Dateien des alten Tools (<code>media</code>)</label>
        <input id="import-media" type="text" value={media} placeholder="/…/data/media" onInput={(e) => setMedia(e.currentTarget.value)} />
      </div>
      {error && <p class="error" role="alert">{error}</p>}
      <div class="row">
        <button type="submit" disabled={busy || db.trim() === '' || media.trim() === ''}>{busy ? 'Importiere …' : 'Importieren'}</button>
      </div>
      {report && (
        <div class="import-report" role="status" data-testid="import-report">
          <p>
            Stammdaten: {report.stammdaten}. Reisen: {report.trips_imported} importiert, {report.trips_skipped} übersprungen. Anhänge:{' '}
            {report.attachments_imported} importiert, {report.attachments_skipped} übersprungen. Dateien kopiert: {report.files_copied}.
          </p>
          {report.warnings.length > 0 && (
            <>
              <strong>Hinweise</strong>
              <ul>{report.warnings.map((w) => <li key={w}>{w}</li>)}</ul>
            </>
          )}
          {report.errors.length > 0 && (
            <>
              <strong class="error">Fehler (diese Einträge wurden nicht importiert)</strong>
              <ul class="error">{report.errors.map((w) => <li key={w}>{w}</li>)}</ul>
            </>
          )}
        </div>
      )}
    </form>
  );
}
