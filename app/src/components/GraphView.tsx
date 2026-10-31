import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import { buildGraph, layout, neighbors } from '../graph';
import type { KindDef, RecordView } from '../types';

interface Props {
  kinds: KindDef[];
  revision: number;
  onOpen: (kind: string, id: string) => void;
}

const WIDTH = 1000;
const HEIGHT = 700;
const MIN_ZOOM = 0.3;
const MAX_ZOOM = 6;

interface ViewState {
  x: number;
  y: number;
  k: number;
}

const INITIAL_VIEW: ViewState = { x: 0, y: 0, k: 1 };

/** Graph aller Einträge nach Verweisen, ähnlich der Graph-Ansicht von SilverBullet. */
export function GraphView({ kinds, revision, onOpen }: Props) {
  const [records, setRecords] = useState<RecordView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<ViewState>(INITIAL_VIEW);
  const svgRef = useRef<SVGSVGElement>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    api.listAll().then(
      (list) => {
        setRecords(list);
        setError(null);
      },
      (e) => setError(messageOf(e)),
    );
  }, [revision]);

  const graph = useMemo(
    () => buildGraph(records.filter((r) => !hidden.has(r.kind)), kinds),
    [records, kinds, hidden],
  );
  const positions = useMemo(() => layout(graph, { width: WIDTH, height: HEIGHT }), [graph]);
  const kindIndex = useMemo(() => new Map(kinds.map((k, i) => [k.kind, i])), [kinds]);
  const kindLabel = (kind: string) => kinds.find((k) => k.kind === kind)?.label ?? kind;

  const selectedNode = graph.nodes.find((n) => n.id === selected) ?? null;
  const neighborIds = useMemo(() => new Set(selectedNode ? neighbors(graph, selectedNode.id) : []), [graph, selectedNode]);
  const q = query.trim().toLowerCase();
  const matching = useMemo(
    () => new Set(q === '' ? [] : graph.nodes.filter((n) => n.label.toLowerCase().includes(q)).map((n) => n.id)),
    [graph, q],
  );

  // Mausrad zoomt um die Zeigerposition. Der Listener muss „nicht passiv“ sein, damit die Seite nicht scrollt.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = svg.getBoundingClientRect();
      const px = ((event.clientX - rect.left) * WIDTH) / rect.width;
      const py = ((event.clientY - rect.top) * HEIGHT) / rect.height;
      setView((v) => {
        const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.k * Math.exp(-event.deltaY * 0.0015)));
        return { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k };
      });
    };
    svg.addEventListener('wheel', onWheel, { passive: false });
    return () => svg.removeEventListener('wheel', onWheel);
  }, []);

  function zoomBy(factor: number) {
    setView((v) => {
      const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.k * factor));
      const cx = WIDTH / 2;
      const cy = HEIGHT / 2;
      return { k, x: cx - ((cx - v.x) * k) / v.k, y: cy - ((cy - v.y) * k) / v.k };
    });
  }

  function onPointerDown(event: PointerEvent) {
    if ((event.target as Element).closest('.node')) return;
    drag.current = { x: event.clientX, y: event.clientY };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    const start = drag.current;
    const svg = svgRef.current;
    if (!start || !svg) return;
    const rect = svg.getBoundingClientRect();
    const dx = ((event.clientX - start.x) * WIDTH) / rect.width;
    const dy = ((event.clientY - start.y) * HEIGHT) / rect.height;
    drag.current = { x: event.clientX, y: event.clientY };
    setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
  }

  function toggleKind(kind: string) {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
    setSelected(null);
  }

  const dimmed = (id: string) => {
    if (q !== '') return !matching.has(id);
    if (selectedNode) return id !== selectedNode.id && !neighborIds.has(id);
    return false;
  };

  return (
    <section class="graph" aria-label="Graph">
      <div class="graph-toolbar">
        <h2>Graph</h2>
        <input type="search" placeholder="Knoten hervorheben …" aria-label="Im Graph suchen" value={query} onInput={(e) => setQuery(e.currentTarget.value)} />
        <div class="legend" role="group" aria-label="Typen anzeigen">
          {kinds.map((k, i) => (
            <label class="check" key={k.kind}>
              <input type="checkbox" checked={!hidden.has(k.kind)} onChange={() => toggleKind(k.kind)} />
              <span class={`dot kind-${i % 8}`} aria-hidden="true" />
              {k.label_plural}
            </label>
          ))}
        </div>
        <div class="row">
          <button type="button" aria-label="Vergrößern" onClick={() => zoomBy(1.3)}>+</button>
          <button type="button" aria-label="Verkleinern" onClick={() => zoomBy(1 / 1.3)}>−</button>
          <button type="button" onClick={() => setView(INITIAL_VIEW)}>Ansicht zurücksetzen</button>
        </div>
      </div>
      {error && <p class="error" role="alert">{error}</p>}

      <div class="graph-body">
        <svg
          ref={svgRef}
          viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
          role="group"
          aria-label="Graph der Einträge. Ziehen verschiebt, Mausrad zoomt."
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={() => (drag.current = null)}
          onPointerCancel={() => (drag.current = null)}
          onClick={(e) => {
            if (!(e.target as Element).closest('.node')) setSelected(null);
          }}
        >
          <g transform={`translate(${view.x} ${view.y}) scale(${view.k})`}>
            {graph.edges.map((e) => {
              const a = positions.get(e.source);
              const b = positions.get(e.target);
              if (!a || !b) return null;
              const active = selectedNode && (e.source === selectedNode.id || e.target === selectedNode.id);
              return (
                <line
                  key={`${e.source}-${e.target}`}
                  data-testid="graph-edge"
                  class={`edge${active ? ' active' : ''}${dimmed(e.source) || dimmed(e.target) ? ' dimmed' : ''}`}
                  x1={a.x}
                  y1={a.y}
                  x2={b.x}
                  y2={b.y}
                />
              );
            })}
            {graph.nodes.map((n) => {
              const p = positions.get(n.id);
              if (!p) return null;
              const classes = ['node', `kind-${(kindIndex.get(n.kind) ?? 0) % 8}`];
              if (n.id === selected) classes.push('selected');
              if (matching.has(n.id)) classes.push('match');
              if (dimmed(n.id)) classes.push('dimmed');
              return (
                <g
                  key={n.id}
                  class={classes.join(' ')}
                  data-testid="graph-node"
                  data-id={n.id}
                  data-kind={n.kind}
                  transform={`translate(${p.x} ${p.y})`}
                  tabIndex={0}
                  role="button"
                  aria-label={`${kindLabel(n.kind)}: ${n.label}`}
                  aria-pressed={n.id === selected}
                  onClick={() => setSelected(n.id)}
                  onDblClick={() => onOpen(n.kind, n.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      setSelected(n.id);
                    }
                  }}
                >
                  <circle class="hit" r={22} />
                  <circle r={n.kind === 'schule' ? 13 : 10} />
                  <text y={26} text-anchor="middle">{n.label}</text>
                </g>
              );
            })}
          </g>
        </svg>

        <aside class="graph-panel" aria-label="Auswahl">
          {graph.nodes.length === 0 && <p class="muted">Noch keine Einträge. Lege Schulen und Ansprechpartner an und verknüpfe sie.</p>}
          {graph.nodes.length > 0 && !selectedNode && (
            <p class="muted">{graph.nodes.length} Einträge, {graph.edges.length} Verbindungen. Wähle einen Knoten, um seine Verbindungen zu sehen.</p>
          )}
          {selectedNode && (
            <>
              <h3>{selectedNode.label}</h3>
              <p class="muted">{kindLabel(selectedNode.kind)}</p>
              <button type="button" class="primary" onClick={() => onOpen(selectedNode.kind, selectedNode.id)}>Bearbeiten</button>
              <h4>Verbunden mit</h4>
              {neighborIds.size === 0 && <p class="muted">Keine Verbindungen.</p>}
              <ul>
                {[...neighborIds].map((id) => {
                  const node = graph.nodes.find((x) => x.id === id);
                  return node ? (
                    <li key={id}>
                      <button type="button" class="link" onClick={() => setSelected(id)}>
                        {node.label} <span class="muted">({kindLabel(node.kind)})</span>
                      </button>
                    </li>
                  ) : null;
                })}
              </ul>
            </>
          )}
        </aside>
      </div>
    </section>
  );
}
