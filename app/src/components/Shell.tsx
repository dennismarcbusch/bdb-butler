import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import { overdueCount, todayIso } from '../auftrag';
import { messageOf } from '../errors';
import type { KindDef, RecordView } from '../types';
import { useSync } from '../useSync';
import { Conflicts } from './Conflicts';
import { GraphView } from './GraphView';
import { ReportView } from './ReportView';
import { KindView } from './KindView';
import { Settings } from './Settings';

interface Props {
  configured: boolean;
  onStatusChanged: () => void;
}

/** Nach dieser Zeit ohne Eingabe sperrt sich die App. */
const IDLE_LOCK_MS = 15 * 60 * 1000;

export function Shell({ configured, onStatusChanged }: Props) {
  const [kinds, setKinds] = useState<KindDef[] | null>(null);
  const [view, setView] = useState('schule');
  // Eintrag, der beim Wechsel in eine Typansicht ausgewählt werden soll. `nonce` erzwingt ein Neuladen.
  const [focus, setFocus] = useState<{ kind: string; id: string; nonce: number } | null>(null);
  const [conflicts, setConflicts] = useState<RecordView[]>([]);
  const [overdue, setOverdue] = useState(0);
  const [openRequests, setOpenRequests] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const { state: sync, run: runSync, refill: refillServer, schedule } = useSync(configured);

  const loadConflicts = useCallback(async () => {
    try {
      setOverdue(overdueCount(await api.list('auftrag'), todayIso()));
      setOpenRequests((await api.list('bedarf')).filter((b) => b.fields.status === 'Offen').length);
      setConflicts(await api.conflicts());
    } catch (e) {
      setError(messageOf(e));
    }
  }, []);

  useEffect(() => {
    api.kinds().then(setKinds, (e) => setError(messageOf(e)));
  }, []);

  // Auch beim Wechsel der Ansicht neu laden: Einträge können von außen entstehen (KI-Zugriff, anderer Rechner).
  useEffect(() => {
    void loadConflicts();
  }, [loadConflicts, sync.revision, view]);

  useEffect(() => {
    if (configured) void runSync();
  }, [configured, runSync]);

  const lock = useCallback(async () => {
    await api.lock();
    onStatusChanged();
  }, [onStatusChanged]);

  // Automatisch sperren, wenn eine Weile nichts eingegeben wurde.
  const idleTimer = useRef<number | undefined>(undefined);
  useEffect(() => {
    const reset = () => {
      window.clearTimeout(idleTimer.current);
      idleTimer.current = window.setTimeout(() => void lock(), IDLE_LOCK_MS);
    };
    const events = ['pointerdown', 'keydown', 'wheel'];
    events.forEach((name) => window.addEventListener(name, reset));
    reset();
    return () => {
      events.forEach((name) => window.removeEventListener(name, reset));
      window.clearTimeout(idleTimer.current);
    };
  }, [lock]);

  const openRecord = useCallback((kind: string, id: string) => {
    setFocus((f) => ({ kind, id, nonce: (f?.nonce ?? 0) + 1 }));
    setView(kind);
  }, []);

  const afterChange = useCallback(() => {
    void loadConflicts();
    schedule();
  }, [loadConflicts, schedule]);

  if (!kinds) {
    return <main class="center">{error ? <p class="error" role="alert">{error}</p> : <p>Lade …</p>}</main>;
  }

  const activeKind = kinds.find((k) => k.kind === view);

  return (
    <div class="shell">
      <header>
        <h1>BdB-Butler</h1>
        <nav aria-label="Bereiche">
          {kinds.filter((k) => !k.hidden).map((k) => (
            <button key={k.kind} type="button" class={view === k.kind ? 'active' : ''} onClick={() => setView(k.kind)}>
              {k.label_plural}
              {k.kind === 'auftrag' && overdue > 0 ? ` (${overdue} überfällig)` : ''}
              {k.kind === 'bedarf' && openRequests > 0 ? ` (${openRequests} offen)` : ''}
            </button>
          ))}
          <button type="button" class={view === 'graph' ? 'active' : ''} onClick={() => setView('graph')}>
            Graph
          </button>
          <button type="button" class={view === 'report' ? 'active' : ''} onClick={() => setView('report')}>
            Tätigkeitsbericht
          </button>
          <button type="button" class={view === 'conflicts' ? 'active' : ''} onClick={() => setView('conflicts')}>
            Konflikte{conflicts.length > 0 ? ` (${conflicts.length})` : ''}
          </button>
          <button type="button" class={view === 'settings' ? 'active' : ''} onClick={() => setView('settings')}>
            Einstellungen
          </button>
        </nav>
        <div class="header-actions">
          <span class="sync-status" role="status" data-testid="sync-status">
            {!configured
              ? 'Kein Server'
              : sync.busy
                ? 'Gleiche ab …'
                : sync.error
                  ? 'Abgleich fehlgeschlagen'
                  : sync.warnings.length > 0
                    ? 'Abgleich mit Hinweisen (siehe Einstellungen)'
                  : sync.lastSync
                    ? `Abgeglichen ${sync.lastSync.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' })}`
                    : ''}
          </span>
          <button type="button" onClick={() => void lock()}>Sperren</button>
        </div>
      </header>

      {sync.error && <p class="banner error" role="alert">{sync.error}</p>}
      {error && <p class="banner error" role="alert">{error}</p>}

      <main class="content">
        {activeKind && (
          <KindView
            key={`${activeKind.kind}:${focus?.kind === activeKind.kind ? focus.nonce : 0}`}
            kind={activeKind}
            kinds={kinds}
            revision={sync.revision}
            initialSelected={focus?.kind === activeKind.kind ? focus.id : null}
            onChanged={afterChange}
            onNavigate={openRecord}
          />
        )}
        {view === 'report' && <ReportView revision={sync.revision} />}
        {view === 'graph' && <GraphView kinds={kinds} revision={sync.revision} onOpen={openRecord} />}
        {view === 'conflicts' && (
          <Conflicts kinds={kinds} conflicts={conflicts} onChanged={afterChange} />
        )}
        {view === 'settings' && (
          <Settings
            kinds={kinds}
            configured={configured}
            sync={sync}
            onSync={() => void runSync()}
            onRefill={refillServer}
            onStatusChanged={onStatusChanged}
          />
        )}
      </main>
    </div>
  );
}
