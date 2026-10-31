import { useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import type { Config, KindDef } from '../types';
import type { SyncState } from '../useSync';
import { McpSettings } from './McpSettings';
import { ImportSettings } from './ImportSettings';
import { StammdatenSettings } from './StammdatenSettings';

interface Props {
  kinds: KindDef[];
  configured: boolean;
  sync: SyncState;
  onSync: () => void;
  onStatusChanged: () => void;
}

const MIN_PASSWORD = 10;

export function Settings({ kinds, configured, sync, onSync, onStatusChanged }: Props) {
  const [config, setConfig] = useState<Config | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [oldPassword, setOldPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [passwordMessage, setPasswordMessage] = useState<string | null>(null);
  const [passwordError, setPasswordError] = useState<string | null>(null);

  useEffect(() => {
    api.configGet().then(setConfig, (e) => setError(messageOf(e)));
  }, []);

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
        <button type="button" disabled={!configured || sync.busy} onClick={onSync}>
          {sync.busy ? 'Gleiche ab …' : 'Jetzt abgleichen'}
        </button>
      </div>

      <StammdatenSettings kinds={kinds} />

      <ImportSettings />

      <McpSettings kinds={kinds} />

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
  );
}
