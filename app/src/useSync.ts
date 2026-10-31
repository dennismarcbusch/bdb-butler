import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import { api, BackendError } from './api';
import { messageOf } from './errors';
import type { SyncReport } from './types';

export interface SyncState {
  busy: boolean;
  lastSync: Date | null;
  error: string | null;
  /** Code des Fehlers, zum Beispiel `server_reset`: Der Server passt nicht mehr zu diesem Rechner. */
  errorCode: string | null;
  /** Hinweise des letzten Abgleichs (er lief trotzdem durch). */
  warnings: string[];
  /// Wird bei jedem Abgleich erhöht, der lokale Daten verändert hat. Listen laden dann neu.
  revision: number;
}

const DEBOUNCE_MS = 1500;

/**
 * Abgleich mit dem Server: sofort (`run`) oder kurz nach einer Änderung (`schedule`). `refill` setzt diesen Rechner
 * zurück und befüllt den Server neu (nur nach Bestätigung aufrufen, siehe Einstellungen).
 */
export function useSync(configured: boolean) {
  const [state, setState] = useState<SyncState>({ busy: false, lastSync: null, error: null, errorCode: null, warnings: [], revision: 0 });
  const timer = useRef<number | undefined>(undefined);
  const running = useRef(false);

  const execute = useCallback(async (action: () => Promise<SyncReport>): Promise<SyncReport | null> => {
    if (!configured || running.current) return null;
    running.current = true;
    setState((s) => ({ ...s, busy: true, error: null, errorCode: null }));
    try {
      const report = await action();
      const changed = report.pulled > 0;
      setState((s) => ({
        busy: false,
        lastSync: new Date(),
        error: null,
        errorCode: null,
        warnings: report.warnings ?? [],
        revision: changed ? s.revision + 1 : s.revision,
      }));
      return report;
    } catch (e) {
      const code = e instanceof BackendError ? e.code : null;
      setState((s) => ({ ...s, busy: false, error: messageOf(e), errorCode: code }));
      return null;
    } finally {
      running.current = false;
    }
  }, [configured]);

  const run = useCallback(() => execute(api.sync), [execute]);
  const refill = useCallback(() => execute(api.serverRefill), [execute]);

  const schedule = useCallback(() => {
    if (!configured) return;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void run(), DEBOUNCE_MS);
  }, [configured, run]);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return { state, run, refill, schedule };
}
