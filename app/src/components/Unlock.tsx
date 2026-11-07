import { useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';

export function Unlock({ onUnlocked }: { onUnlocked: () => void }) {
  const [useRecovery, setUseRecovery] = useState(false);
  const [secret, setSecret] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: Event) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (useRecovery) await api.unlockRecovery(secret);
      else await api.unlock(secret);
      setSecret('');
      onUnlocked();
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
        <p class="muted">Die App ist gesperrt.</p>
        <label>
          {useRecovery ? 'Recovery-Key' : 'Master-Passwort'}
          <input
            type={useRecovery ? 'text' : 'password'}
            autocomplete={useRecovery ? 'off' : 'current-password'}
            autofocus
            value={secret}
            onInput={(e) => setSecret(e.currentTarget.value)}
          />
        </label>
        {error && <p class="error" role="alert">{error}</p>}
        <button type="submit" class="primary" disabled={busy || secret === ''}>
          {busy ? 'Entsperre …' : 'Entsperren'}
        </button>
        <button type="button" class="link" onClick={() => { setUseRecovery(!useRecovery); setSecret(''); setError(null); }}>
          {useRecovery ? 'Mit Master-Passwort entsperren' : 'Mit Recovery-Key entsperren'}
        </button>
      </form>
    </main>
  );
}
