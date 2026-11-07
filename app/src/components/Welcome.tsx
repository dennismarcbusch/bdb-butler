import { useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';

interface Props {
  onCreated: (recoveryKey: string) => void;
  onJoined: () => void;
}

const MIN_PASSWORD = 10;

export function Welcome({ onCreated, onJoined }: Props) {
  const [mode, setMode] = useState<'create' | 'join'>('create');
  const [serverUrl, setServerUrl] = useState('');
  const [token, setToken] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const joining = mode === 'join';

  async function submit(event: Event) {
    event.preventDefault();
    setError(null);
    if (password.length < MIN_PASSWORD) {
      setError(`Das Master-Passwort muss mindestens ${MIN_PASSWORD} Zeichen haben.`);
      return;
    }
    if (!joining && password !== confirm) {
      setError('Die beiden Passwörter stimmen nicht überein.');
      return;
    }
    if (joining && (serverUrl.trim() === '' || token.trim() === '')) {
      setError('Zum Verbinden braucht es Server-Adresse und Token.');
      return;
    }
    setBusy(true);
    try {
      if (serverUrl.trim() !== '' || token.trim() !== '') {
        await api.configSet({ server_url: serverUrl, token, attachments_dir: (await api.configGet()).attachments_dir });
      }
      if (joining) {
        await api.joinVault(password);
        onJoined();
      } else {
        const { recovery_key } = await api.createVault(password);
        onCreated(recovery_key);
      }
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main class="center">
      <form class="card" onSubmit={submit}>
        <h1 class="brand">BdB-Butler</h1>
        <p class="muted">
          Alle Daten werden auf diesem Rechner mit deinem Master-Passwort verschlüsselt. Der Server sieht nur Chiffretext.
        </p>

        <div class="tabs" role="tablist">
          <button type="button" role="tab" aria-selected={!joining} class={!joining ? 'active' : ''} onClick={() => setMode('create')}>
            Neu einrichten
          </button>
          <button type="button" role="tab" aria-selected={joining} class={joining ? 'active' : ''} onClick={() => setMode('join')}>
            Mit vorhandenem Tresor verbinden
          </button>
        </div>

        <fieldset>
          <legend>Server{joining ? '' : ' (kann später eingetragen werden)'}</legend>
          <label>
            Server-Adresse
            <input type="url" value={serverUrl} placeholder="https://butler.example.de" onInput={(e) => setServerUrl(e.currentTarget.value)} />
          </label>
          <label>
            Token
            <input type="password" autocomplete="off" value={token} onInput={(e) => setToken(e.currentTarget.value)} />
          </label>
        </fieldset>

        <label>
          Master-Passwort
          <input type="password" autocomplete={joining ? 'current-password' : 'new-password'} value={password} onInput={(e) => setPassword(e.currentTarget.value)} />
        </label>
        {!joining && (
          <label>
            Master-Passwort wiederholen
            <input type="password" autocomplete="new-password" value={confirm} onInput={(e) => setConfirm(e.currentTarget.value)} />
          </label>
        )}
        {!joining && (
          <p class="muted">Am besten eine lange Passphrase aus mehreren zufälligen Wörtern (mindestens {MIN_PASSWORD} Zeichen).</p>
        )}

        {error && <p class="error" role="alert">{error}</p>}
        <button type="submit" class="primary" disabled={busy}>
          {busy ? 'Einen Moment …' : joining ? 'Verbinden' : 'Tresor anlegen'}
        </button>
      </form>
    </main>
  );
}
