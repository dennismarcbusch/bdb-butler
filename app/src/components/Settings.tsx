import { useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import type { Config, KindDef, SyncReport } from '../types';
import type { SyncState } from '../useSync';
import { McpSettings } from './McpSettings';
import { StammdatenSettings } from './StammdatenSettings';

interface Props {
  kinds: KindDef[];
  configured: boolean;
  sync: SyncState;
  onSync: () => void;
  /** „Server neu befüllen“ (nach Bestätigung). Liefert den Bericht des anschließenden Abgleichs oder `null`. */
  onRefill: () => Promise<SyncReport | null>;
  onStatusChanged: () => void;
}

const MIN_PASSWORD = 10;

/** Abschnitte der Seite, in der Reihenfolge von oben nach unten. */
const SECTIONS = [
  { id: 'einstellungen-abgleich', label: 'Server und Abgleich' },
  { id: 'einstellungen-stammdaten', label: 'Stammdaten' },
  { id: 'einstellungen-ki', label: 'KI-Zugriff' },
  { id: 'einstellungen-sicherheit', label: 'Sicherheit' },
] as const;

export function Settings({ kinds, configured, sync, onSync, onRefill, onStatusChanged }: Props) {
  const [config, setConfig] = useState<Config | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [oldPassword, setOldPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [passwordMessage, setPasswordMessage] = useState<string | null>(null);
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const [confirmRefill, setConfirmRefill] = useState(false);
  const [refillMessage, setRefillMessage] = useState<string | null>(null);

  const [activeSection, setActiveSection] = useState<string>(SECTIONS[0].id);
  const loaded = config !== null;

  useEffect(() => {
    api.configGet().then(setConfig, (e) => setError(messageOf(e)));
  }, []);

  // Hervorhebung im Abschnittsmenü folgt dem Scrollen: aktiv ist der oberste Abschnitt, der im Sichtfenster beginnt.
  useEffect(() => {
    if (!loaded || typeof IntersectionObserver === 'undefined') return;
    const visible = new Set<string>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) visible.add(entry.target.id);
          else visible.delete(entry.target.id);
        }
        const first = SECTIONS.find((s) => visible.has(s.id));
        if (first) setActiveSection(first.id);
      },
      { rootMargin: '0px 0px -60% 0px' },
    );
    for (const s of SECTIONS) {
      const el = document.getElementById(s.id);
      if (el) observer.observe(el);
    }
    return () => observer.disconnect();
  }, [loaded]);

  function goTo(id: string) {
    setActiveSection(id);
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  if (!config) return <p>{error ?? 'Lade …'}</p>;

  const update = (patch: Partial<Config>) => setConfig({ ...config, ...patch });

  async function save(event: Event) {
    event.preventDefault();
    setError(null);
    setMessage(null);
    try {
      await api.configSet(config!);
      setConfig(await api.configGet());
      setMessage('Gespeichert.');
      onStatusChanged();
    } catch (e) {
      setError(messageOf(e));
    }
  }

  async function refill() {
    setRefillMessage(null);
    const report = await onRefill();
    setConfirmRefill(false);
    if (report) {
      setRefillMessage(
        'Der Server ist wieder in Betrieb. Auf den anderen Rechnern beim nächsten Abgleich ebenfalls „Server neu befüllen“ wählen.',
      );
    }
  }

  async function changePassword(event: Event) {
    event.preventDefault();
    setPasswordError(null);
    setPasswordMessage(null);
    if (newPassword.length < MIN_PASSWORD) {
      setPasswordError(`Das neue Passwort muss mindestens ${MIN_PASSWORD} Zeichen haben.`);
      return;
    }
    if (newPassword !== confirm) {
      setPasswordError('Die beiden neuen Passwörter stimmen nicht überein.');
      return;
    }
    try {
      await api.changePassword(oldPassword, newPassword);
      setOldPassword('');
      setNewPassword('');
      setConfirm('');
      setPasswordMessage('Das Master-Passwort wurde geändert. Der Recovery-Key bleibt gültig.');
    } catch (e) {
      setPasswordError(messageOf(e));
    }
  }

  return (
    <section class="settings">
      <h2>Einstellungen</h2>

      <div class="settings-layout">
        <div class="settings-nav" role="group" aria-label="Abschnitte der Einstellungen">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              class={activeSection === s.id ? 'active' : ''}
              aria-current={activeSection === s.id ? 'true' : undefined}
              onClick={() => goTo(s.id)}
            >
              {s.label}
            </button>
          ))}
        </div>

        <div class="settings-sections">
      <section id="einstellungen-abgleich" class="settings-section" aria-label="Server und Abgleich">
      <form onSubmit={save}>
        <h3>Server und Ordner</h3>
        <label>
          Server-Adresse
          <input type="url" value={config.server_url} placeholder="https://butler.example.de" onInput={(e) => update({ server_url: e.currentTarget.value })} />
        </label>
        <label>
          Token
          <input type="password" autocomplete="off" value={config.token} onInput={(e) => update({ token: e.currentTarget.value })} />
        </label>
        <label>
          Ordner für PDFs und Anhänge
          <input type="text" value={config.attachments_dir} placeholder="/home/…/Nextcloud/BdB" onInput={(e) => update({ attachments_dir: e.currentTarget.value })} />
        </label>
        <p class="muted">Der Ordner gilt nur für diesen Rechner. Nextcloud oder ein anderes Werkzeug synchronisiert ihn.</p>
        {error && <p class="error" role="alert">{error}</p>}
        {message && <p class="ok" role="status">{message}</p>}
        <div class="row">
          <button type="submit" class="primary">Speichern</button>
        </div>
      </form>

      <div class="block">
        <h3>Abgleich</h3>
        <p class="muted" role="status">
          {!configured
            ? 'Trage Server und Token ein, um mit anderen Rechnern abzugleichen.'
            : sync.error
              ? sync.error
              : sync.lastSync
                ? `Zuletzt abgeglichen um ${sync.lastSync.toLocaleTimeString('de-DE')}.`
                : 'Noch nicht abgeglichen.'}
        </p>
        {sync.warnings.map((w) => (
          <p key={w} class="error" role="alert">{w}</p>
        ))}
        {refillMessage && <p class="ok" role="status">{refillMessage}</p>}
        <button type="button" disabled={!configured || sync.busy} onClick={onSync}>
          {sync.busy ? 'Gleiche ab …' : 'Jetzt abgleichen'}
        </button>
        {configured && sync.errorCode === 'server_reset' && (
          <div class="block" role="region" aria-label="Server neu befüllen">
            <h4>Server neu befüllen</h4>
            <p class="muted">
              Dieser Rechner hat den vollständigen Stand. Er vergisst, was er über die Folgenummern des Servers weiß, und
              sendet alle Einträge erneut. Liegt auf dem Server schon ein Stand (eine ältere Sicherung oder der eines
              anderen Rechners, der schon neu befüllt hat), werden beide je Feld zusammengeführt: Der jüngere Stand
              gewinnt, bei einer älteren Sicherung also der Stand der Rechner. Lokal geht nichts verloren. Danach müssen alle
              anderen Rechner ebenfalls „Server neu befüllen“ wählen.
            </p>
            {!confirmRefill ? (
              <div class="row">
                <button type="button" disabled={sync.busy} onClick={() => setConfirmRefill(true)}>
                  Server neu befüllen …
                </button>
              </div>
            ) : (
              <div class="row">
                <button type="button" class="danger" disabled={sync.busy} onClick={() => void refill()}>
                  Ja, Server neu befüllen
                </button>
                <button type="button" disabled={sync.busy} onClick={() => setConfirmRefill(false)}>
                  Abbrechen
                </button>
              </div>
            )}
          </div>
        )}
      </div>

      </section>

      <section id="einstellungen-stammdaten" class="settings-section">
        <StammdatenSettings kinds={kinds} />
      </section>

      <section id="einstellungen-ki" class="settings-section">
        <McpSettings kinds={kinds} />
      </section>

      <section id="einstellungen-sicherheit" class="settings-section" aria-label="Sicherheit">
      <form onSubmit={changePassword}>
        <h3>Master-Passwort ändern</h3>
        <label>
          Aktuelles Passwort
          <input type="password" autocomplete="current-password" value={oldPassword} onInput={(e) => setOldPassword(e.currentTarget.value)} />
        </label>
        <label>
          Neues Passwort
          <input type="password" autocomplete="new-password" value={newPassword} onInput={(e) => setNewPassword(e.currentTarget.value)} />
        </label>
        <label>
          Neues Passwort wiederholen
          <input type="password" autocomplete="new-password" value={confirm} onInput={(e) => setConfirm(e.currentTarget.value)} />
        </label>
        {passwordError && <p class="error" role="alert">{passwordError}</p>}
        {passwordMessage && <p class="ok" role="status">{passwordMessage}</p>}
        <div class="row">
          <button type="submit">Passwort ändern</button>
        </div>
      </form>
      </section>
        </div>
      </div>
    </section>
  );
}
