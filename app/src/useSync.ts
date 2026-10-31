import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import { api } from './api';
import { messageOf } from './errors';
import type { SyncReport } from './types';

export interface SyncState {
  busy: boolean;
  lastSync: Date | null;
  error: string | null;
  /// Wird bei jedem Abgleich erhöht, der lokale Daten verändert hat. Listen laden dann neu.
  revision: number;
}

const DEBOUNCE_MS = 1500;

/** Abgleich mit dem Server: sofort (`run`) oder kurz nach einer Änderung (`schedule`). */
export function useSync(configured: boolean) {
  const [state, setState] = useState<SyncState>({ busy: false, lastSync: null, error: null, revision: 0 });
  const timer = useRef<number | undefined>(undefined);
  const running = useRef(false);

  const run = useCallback(async (): Promise<SyncReport | null> => {
    if (!configured || running.current) return null;
    running.current = true;
    setState((s) => ({ ...s, busy: true, error: null }));
    try {
      const report = await api.sync();
      const changed = report.pulled > 0;
      setState((s) => ({ busy: false, lastSync: new Date(), error: null, revision: changed ? s.revision + 1 : s.revision }));
      return report;
    } catch (e) {
      setState((s) => ({ ...s, busy: false, error: messageOf(e) }));
      return null;
    } finally {
      running.current = false;
    }
  }, [configured]);

  const schedule = useCallback(() => {
    if (!configured) return;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void run(), DEBOUNCE_MS);
  }, [configured, run]);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return { state, run, schedule };
}
