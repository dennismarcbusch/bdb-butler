import { useState } from 'preact/hooks';
import { splitRecoveryKey } from '../form';

interface Props {
  recoveryKey: string;
  onConfirmed: () => void;
}

export function RecoveryKeyScreen({ recoveryKey, onConfirmed }: Props) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(recoveryKey);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <main class="center">
      <div class="card">
        <h1>Recovery-Key</h1>
        <p>
          Mit diesem Schlüssel kommst du an deine Daten, wenn du das Master-Passwort vergisst. Er wird <strong>nur jetzt</strong> angezeigt
          und nirgends gespeichert. Drucke ihn aus und bewahre ihn offline auf.
        </p>
        <p class="recovery-key" data-testid="recovery-key" aria-label="Recovery-Key">
          {splitRecoveryKey(recoveryKey).map((part) => (
            <span key={part}>{part}</span>
          ))}
        </p>
        <div class="row">
          <button type="button" onClick={() => void copy()}>{copied ? 'Kopiert' : 'Kopieren'}</button>
          <button type="button" onClick={() => window.print()}>Drucken</button>
        </div>
        <label class="check">
          <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.currentTarget.checked)} />
          Ich habe den Recovery-Key sicher aufbewahrt.
        </label>
        <button type="button" class="primary" disabled={!saved} onClick={onConfirmed}>Weiter</button>
      </div>
    </main>
  );
}
