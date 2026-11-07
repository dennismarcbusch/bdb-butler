import { useCallback, useEffect, useState } from 'preact/hooks';
import { api } from './api';
import { messageOf } from './errors';
import type { Status } from './types';
import { RecoveryKeyScreen } from './components/RecoveryKeyScreen';
import { Shell } from './components/Shell';
import { Unlock } from './components/Unlock';
import { Welcome } from './components/Welcome';

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  // Der Recovery-Key wird nur direkt nach dem Anlegen angezeigt und nirgends gespeichert.
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.status());
      setFatal(null);
    } catch (e) {
      setFatal(messageOf(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (fatal) {
    return (
      <main class="center">
        <div class="card">
          <h1 class="brand">BdB-Butler</h1>
          <p class="error" role="alert">{fatal}</p>
          <button type="button" onClick={() => void refresh()}>Erneut versuchen</button>
        </div>
      </main>
    );
  }
  if (!status) {
    return <main class="center"><p>Lade …</p></main>;
  }
  if (recoveryKey) {
    return (
      <RecoveryKeyScreen
        recoveryKey={recoveryKey}
        onConfirmed={() => {
          setRecoveryKey(null);
          void refresh();
        }}
      />
    );
  }
  if (!status.has_vault) {
    return (
      <Welcome
        onCreated={(key) => {
          setRecoveryKey(key);
          void refresh();
        }}
        onJoined={() => void refresh()}
      />
    );
  }
  if (!status.unlocked) {
    return <Unlock onUnlocked={() => void refresh()} />;
  }
  return <Shell configured={status.configured} onStatusChanged={() => void refresh()} />;
}
