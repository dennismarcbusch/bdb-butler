import { useCallback, useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import type { AuditEntry, KindDef, McpState } from '../types';

/** Einstellungen des lokalen MCP-Servers (KI-Zugriff). */
export function McpSettings({ kinds }: { kinds: KindDef[] }) {
  const [state, setState] = useState<McpState | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [port, setPort] = useState('8765');
  const [write, setWrite] = useState(false);
  const [allowed, setAllowed] = useState<string[]>([]);
  const [showToken, setShowToken] = useState(false);
  const [confirmToken, setConfirmToken] = useState(false);
  const [copied, setCopied] = useState(false);
  const [audit, setAudit] = useState<AuditEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const personal = kinds.filter((k) => k.personal);

  const apply = useCallback((s: McpState) => {
    setState(s);
    setEnabled(s.enabled);
    setPort(String(s.port));
    setWrite(s.write);
    setAllowed(s.allow_personal);
  }, []);

  const loadAudit = useCallback(async () => {
    try {
      setAudit((await api.mcpAudit(20)).reverse());
    } catch (e) {
      setError(messageOf(e));
    }
  }, []);

  useEffect(() => {
    api.mcpGet().then(apply, (e) => setError(messageOf(e)));
    void loadAudit();
  }, [apply, loadAudit]);

  if (!state) return <p>{error ?? 'Lade …'}</p>;

  async function save(event: Event) {
    event.preventDefault();
    setError(null);
    setMessage(null);
    const portNumber = Number(port);
    if (!Number.isInteger(portNumber) || portNumber < 0 || portNumber > 65535) {
      setError('Der Port muss eine Zahl zwischen 1024 und 65535 sein.');
      return;
    }
    try {
      apply(await api.mcpSet(enabled, portNumber, write, allowed));
      setMessage('Gespeichert.');
    } catch (e) {
      setError(messageOf(e));
      // Die Einstellungen sind gespeichert, auch wenn der Server nicht starten konnte.
      api.mcpGet().then(apply, () => undefined);
    }
  }

  async function newToken() {
    setError(null);
    try {
      apply(await api.mcpNewToken());
      setConfirmToken(false);
      setShowToken(true);
      setMessage('Neues Token erzeugt. Clients mit dem alten Token haben keinen Zugriff mehr.');
    } catch (e) {
      setError(messageOf(e));
    }
  }

  async function copyToken() {
    try {
      await navigator.clipboard.writeText(state!.token);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  const url = state.url ?? `http://127.0.0.1:${port || state.port}/mcp`;
  const command = `claude mcp add --transport http bdb-butler ${url} --header "Authorization: Bearer ${showToken ? state.token : '<Token>'}"`;

  return (
    <form class="mcp" onSubmit={save} aria-label="KI-Zugriff">
      <h3>KI-Zugriff (MCP)</h3>
      <p class="muted">
        Ein lokaler Server in dieser App, den KI-Programme auf <strong>diesem Rechner</strong> nutzen können (zum Beispiel Claude Code oder
        Claude Desktop). Er ist nur erreichbar, solange die App läuft und entsperrt ist. Verarbeite über die KI keine personenbezogenen Daten.
      </p>

      <label class="check">
        <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.currentTarget.checked)} />
        KI-Zugriff einschalten
      </label>
      <label>
        Port
        <input type="text" inputMode="numeric" value={port} onInput={(e) => setPort(e.currentTarget.value)} />
      </label>
      <label class="check">
        <input type="checkbox" checked={write} onChange={(e) => setWrite(e.currentTarget.checked)} />
        Schreibzugriff: die KI darf Einträge anlegen und ändern (löschen und Mails senden kann sie nie)
      </label>

      <fieldset>
        <legend>Typen mit Personenbezug</legend>
        <p class="muted">Diese Typen sind für die KI gesperrt, solange du sie nicht freigibst.</p>
        {personal.map((k) => (
          <label class="check" key={k.kind}>
            <input
              type="checkbox"
              checked={allowed.includes(k.kind)}
              onChange={(e) =>
                setAllowed(e.currentTarget.checked ? [...allowed, k.kind] : allowed.filter((x) => x !== k.kind))
              }
            />
            {k.label_plural} für die KI freigeben
          </label>
        ))}
      </fieldset>

      {error && <p class="error" role="alert">{error}</p>}
      {message && <p class="ok" role="status">{message}</p>}
      <div class="row">
        <button type="submit" class="primary">Speichern</button>
      </div>

      <p role="status" data-testid="mcp-status">
        {state.running ? <>Läuft auf <code>{state.url}</code></> : 'Der KI-Zugriff ist aus.'}
      </p>

      <div class="block">
        <h4>Zugangsdaten</h4>
        <p class="muted">Token (wie ein Passwort behandeln):</p>
        <p>
          <code data-testid="mcp-token">{showToken ? state.token : '•'.repeat(24)}</code>
        </p>
        <div class="row">
          <button type="button" onClick={() => setShowToken(!showToken)}>{showToken ? 'Verbergen' : 'Anzeigen'}</button>
          <button type="button" onClick={() => void copyToken()}>{copied ? 'Kopiert' : 'Kopieren'}</button>
          {!confirmToken && <button type="button" onClick={() => setConfirmToken(true)}>Neues Token erzeugen</button>}
          {confirmToken && (
            <>
              <button type="button" class="danger" onClick={() => void newToken()}>Wirklich neu erzeugen</button>
              <button type="button" onClick={() => setConfirmToken(false)}>Doch nicht</button>
            </>
          )}
        </div>
        <p class="muted">Für Claude Code:</p>
        <pre class="command">{command}</pre>
      </div>

      <div class="block">
        <h4>Protokoll der letzten Zugriffe</h4>
        <p class="muted">Enthält Werkzeug, Typ, Eintrags-ID und Feldnamen, aber nie Inhalte.</p>
        <button type="button" onClick={() => void loadAudit()}>Aktualisieren</button>
        {audit.length === 0 && <p class="muted">Noch keine Zugriffe.</p>}
        {audit.length > 0 && (
          <table class="audit" data-testid="mcp-audit">
            <thead>
              <tr><th>Zeit</th><th>Werkzeug</th><th>Typ</th><th>Felder</th><th>Ergebnis</th></tr>
            </thead>
            <tbody>
              {audit.map((a, i) => (
                <tr key={`${a.t}-${i}`}>
                  <td>{new Date(a.t).toLocaleTimeString('de-DE')}</td>
                  <td>{a.tool}</td>
                  <td>{a.kind ?? ''}</td>
                  <td>{(a.fields ?? []).join(', ')}</td>
                  <td>{a.ok ? 'ok' : `abgelehnt${a.error ? `: ${a.error}` : ''}`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </form>
  );
}
