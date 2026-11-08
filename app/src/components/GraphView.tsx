import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import { Simulation, buildGraph, dropWithOrphans, fitView, neighbors, temperatureFor, type SimulationParams } from '../graph';
import { NO_FILTER, groupMap, matchesFilter, optionsOf, valueOf, type GroupChoice, type SchoolFilter } from '../gruppen';
import type { KindDef, RecordView } from '../types';
import { SchoolControls } from './SchoolControls';

interface Props {
  kinds: KindDef[];
  revision: number;
  onOpen: (kind: string, id: string) => void;
}

const WIDTH = 1000;
const HEIGHT = 700;
const MIN_ZOOM = 0.2;
const MAX_ZOOM = 6;
/// Ab dieser Restwärme ruht die Simulation, bis etwas sie weckt (Ziehen, Regler, geänderter Graph).
const REST_ALPHA = 0.02;
/// So weit (in Bildschirmpunkten) muss der Zeiger wandern, bis aus einem Klick ein Ziehen wird.
const DRAG_THRESHOLD = 4;

interface ViewState {
  x: number;
  y: number;
  k: number;
}

/** Voreinstellungen der Kräfte (siehe `Simulation`). */
const DEFAULT_FORCES: Required<SimulationParams> = { gravity: 0.3, repulsion: 1, linkStrength: 1, spacing: 110, groupPull: 1.5 };

const FORCE_SLIDERS: { key: keyof SimulationParams; label: string; min: number; max: number; step: number }[] = [
  { key: 'gravity', label: 'Mittelkraft', min: 0, max: 1, step: 0.05 },
  { key: 'repulsion', label: 'Abstoßung', min: 0.2, max: 3, step: 0.1 },
  { key: 'linkStrength', label: 'Verbindungskraft', min: 0.2, max: 3, step: 0.1 },
  { key: 'spacing', label: 'Verbindungslänge', min: 50, max: 250, step: 5 },
  { key: 'groupPull', label: 'Gruppenkraft', min: 0, max: 1.9, step: 0.1 },
];

interface Box { x0: number; y0: number; x1: number; y1: number; count: number }

function computeBoxes(sim: Simulation, groups: Map<string, string> | undefined): Map<string, Box> {
  const out = new Map<string, Box>();
  for (const [id, label] of groups ?? []) {
    const p = sim.position(id);
    if (!p) continue;
    const b = out.get(label) ?? { x0: p.x, y0: p.y, x1: p.x, y1: p.y, count: 0 };
    b.x0 = Math.min(b.x0, p.x); b.y0 = Math.min(b.y0, p.y);
    b.x1 = Math.max(b.x1, p.x); b.y1 = Math.max(b.y1, p.y);
    b.count += 1;
    out.set(label, b);
  }
  return out;
}

/** Zeichnet das Rechteck einer Gruppe: etwas größer als die Schulen darin, Platz oben für die Beschriftung. */
function boxAttrs(b: Box) {
  return { x: b.x0 - 38, y: b.y0 - 44, width: b.x1 - b.x0 + 76, height: b.y1 - b.y0 + 96, tx: b.x0 - 28, ty: b.y0 - 26 };
}

/** DOM-Elemente, die die Simulation bei jedem Bild direkt verschiebt (ohne die Oberfläche neu aufzubauen). */
interface Dom {
  nodes: Map<string, Element>;
  edges: { el: Element; source: string; target: string }[];
  boxes: Map<string, Element>;
}

const reducedMotion = () => typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches === true;

/**
 * Graph aller Einträge nach Verweisen, ähnlich der Graph-Ansicht von Obsidian: Die Knoten bewegen sich fortlaufend,
 * lassen sich ziehen (die verbundenen Knoten laufen mit) und kommen danach wieder zur Ruhe. Mausrad zoomt, Ziehen
 * auf dem Hintergrund verschiebt, die Kräfte lassen sich einstellen. Wer Bewegung reduziert haben möchte
 * (Systemeinstellung), bekommt das fertig berechnete Bild.
 */
export function GraphView({ kinds, revision, onOpen }: Props) {
  const [records, setRecords] = useState<RecordView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [group, setGroup] = useState<GroupChoice>('schulart');
  const [filter, setFilter] = useState<SchoolFilter>(NO_FILTER);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [forces, setForces] = useState<Required<SimulationParams>>(DEFAULT_FORCES);

  const svgRef = useRef<SVGSVGElement>(null);
  const rootRef = useRef<SVGGElement>(null);
  const view = useRef<ViewState>({ x: 0, y: 0, k: 1 });
  const autoFit = useRef(true);
  const simRef = useRef<Simulation | null>(null);
  const dom = useRef<Dom | null>(null);
  const alpha = useRef(0);
  const alphaTarget = useRef(0);
  const frame = useRef(0);
  const forcesRef = useRef(forces);
  forcesRef.current = forces;
  const groupsRef = useRef<Map<string, string> | undefined>(undefined);
  const pan = useRef<{ x: number; y: number } | null>(null);
  const nodeDrag = useRef<{ id: string; x: number; y: number; moved: boolean } | null>(null);
  const suppressClick = useRef(false);

  useEffect(() => {
    api.listAll().then(
      (list) => {
        setRecords(list);
        setError(null);
      },
      (e) => setError(messageOf(e)),
    );
  }, [revision]);

  const schoolKind = useMemo(() => kinds.find((k) => k.kind === 'schule'), [kinds]);
  const schools = useMemo(() => records.filter((r) => r.kind === 'schule'), [records]);
  const byId = useMemo(() => new Map(records.map((r) => [r.id, r])), [records]);

  // Erst Typen ausblenden, dann Schulen filtern. Wer nur zu ausgefilterten Schulen gehört, verschwindet mit ihnen.
  const shown = useMemo(() => {
    const byKind = records.filter((r) => !hidden.has(r.kind));
    const removed = new Set(byKind.filter((r) => r.kind === 'schule' && !matchesFilter(r, filter)).map((r) => r.id));
    return dropWithOrphans(byKind, kinds, removed);
  }, [records, kinds, hidden, filter]);
  const graph = useMemo(() => buildGraph(shown, kinds), [shown, kinds]);
  const groups = useMemo(
    () => (group === 'none' ? undefined : groupMap(shown.filter((r) => r.kind === 'schule'), group, optionsOf(schoolKind, group))),
    [shown, group, schoolKind],
  );

  // Eine neue Simulation je Graph. Bestehende Knoten starten an ihrer bisherigen Stelle, das Bild baut sich also
  // nicht neu auf, wenn man einen Filter ändert.
  const sim = useMemo(() => {
    const s = new Simulation(graph, { width: WIDTH, height: HEIGHT, groups, ...forcesRef.current, initial: simRef.current?.positions() });
    if (reducedMotion()) for (let i = 0; i < 300; i++) s.step(temperatureFor(1 - i / 300));
    return s;
  }, [graph, groups]);
  groupsRef.current = groups;
  const boxes = computeBoxes(sim, groups);

  const kindIndex = useMemo(() => new Map(kinds.map((k, i) => [k.kind, i])), [kinds]);
  const kindLabel = (kind: string) => kinds.find((k) => k.kind === kind)?.label ?? kind;

  const selectedNode = graph.nodes.find((n) => n.id === selected) ?? null;
  const focusId = hovered ?? selectedNode?.id ?? null;
  const neighborIds = useMemo(() => new Set(focusId ? neighbors(graph, focusId) : []), [graph, focusId]);
  const q = query.trim().toLowerCase();
  const matching = useMemo(
    () => new Set(q === '' ? [] : graph.nodes.filter((n) => n.label.toLowerCase().includes(q)).map((n) => n.id)),
    [graph, q],
  );

  function applyView() {
    const v = view.current;
    rootRef.current?.setAttribute('transform', `translate(${v.x} ${v.y}) scale(${v.k})`);
  }

  /** Schreibt die Positionen der Simulation in die Elemente. */
  function paint() {
    const s = simRef.current;
    const d = dom.current;
    if (!s || !d) return;
    for (const [id, el] of d.nodes) {
      const p = s.position(id);
      if (p) el.setAttribute('transform', `translate(${p.x} ${p.y})`);
    }
    for (const e of d.edges) {
      const a = s.position(e.source);
      const b = s.position(e.target);
      if (!a || !b) continue;
      e.el.setAttribute('x1', String(a.x));
      e.el.setAttribute('y1', String(a.y));
      e.el.setAttribute('x2', String(b.x));
      e.el.setAttribute('y2', String(b.y));
    }
    for (const [label, b] of computeBoxes(s, groupsRef.current)) {
      const el = d.boxes.get(label);
      if (!el) continue;
      const a = boxAttrs(b);
      const rect = el.querySelector('rect');
      const text = el.querySelector('text');
      rect?.setAttribute('x', String(a.x));
      rect?.setAttribute('y', String(a.y));
      rect?.setAttribute('width', String(a.width));
      rect?.setAttribute('height', String(a.height));
      text?.setAttribute('x', String(a.tx));
      text?.setAttribute('y', String(a.ty));
    }
  }

  function tick() {
    frame.current = 0;
    const s = simRef.current;
    if (!s) return;
    alpha.current = alphaTarget.current + (alpha.current - alphaTarget.current) * 0.985;
    s.step(temperatureFor(alpha.current, WIDTH));
    paint();
    if (autoFit.current) {
      // Die Ansicht folgt dem wachsenden oder schrumpfenden Bild, bis man selbst zoomt oder verschiebt.
      const target = fitView(s.positions().values(), WIDTH, HEIGHT, 70, 1.2);
      const v = view.current;
      view.current = { x: v.x + (target.x - v.x) * 0.12, y: v.y + (target.y - v.y) * 0.12, k: v.k + (target.k - v.k) * 0.12 };
      applyView();
    }
    if (alpha.current > REST_ALPHA || alphaTarget.current > 0) frame.current = requestAnimationFrame(tick);
  }

  /** Weckt die Simulation (mindestens mit der Restwärme `a`). */
  function wake(a: number) {
    alpha.current = Math.max(alpha.current, a);
    if (frame.current === 0 && !reducedMotion()) frame.current = requestAnimationFrame(tick);
  }

  // Neue Simulation übernehmen: Elemente einsammeln, Ansicht einpassen, Bewegung starten.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const hadBefore = simRef.current !== null;
    simRef.current = sim;
    const nodes = new Map<string, Element>();
    svg.querySelectorAll('[data-testid="graph-node"]').forEach((el) => nodes.set(el.getAttribute('data-id') ?? '', el));
    const edges = [...svg.querySelectorAll('[data-testid="graph-edge"]')].map((el) => ({
      el,
      source: el.getAttribute('data-source') ?? '',
      target: el.getAttribute('data-target') ?? '',
    }));
    const boxEls = new Map<string, Element>();
    svg.querySelectorAll('[data-testid="graph-group"]').forEach((el) => boxEls.set(el.getAttribute('data-group') ?? '', el));
    dom.current = { nodes, edges, boxes: boxEls };

    autoFit.current = true;
    view.current = fitView(sim.positions().values(), WIDTH, HEIGHT, 70, 1.2);
    applyView();
    paint();
    wake(hadBefore ? 0.5 : 1);
    return () => {
      if (frame.current !== 0) cancelAnimationFrame(frame.current);
      frame.current = 0;
    };
    // paint und wake lesen nur Refs und sind je Render neu, die Simulation ist der einzige Auslöser.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sim]);

  // Mausrad zoomt um die Zeigerposition. Der Listener muss „nicht passiv“ sein, damit die Seite nicht scrollt.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      autoFit.current = false;
      const rect = svg.getBoundingClientRect();
      const px = ((event.clientX - rect.left) * WIDTH) / rect.width;
      const py = ((event.clientY - rect.top) * HEIGHT) / rect.height;
      const v = view.current;
      const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.k * Math.exp(-event.deltaY * 0.0015)));
      view.current = { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k };
      applyView();
    };
    svg.addEventListener('wheel', onWheel, { passive: false });
    return () => svg.removeEventListener('wheel', onWheel);
  }, []);

  function zoomBy(factor: number) {
    autoFit.current = false;
    const v = view.current;
    const k = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.k * factor));
    const cx = WIDTH / 2;
    const cy = HEIGHT / 2;
    view.current = { k, x: cx - ((cx - v.x) * k) / v.k, y: cy - ((cy - v.y) * k) / v.k };
    applyView();
  }

  function resetView() {
    autoFit.current = true;
    view.current = fitView(sim.positions().values(), WIDTH, HEIGHT, 70, 1.2);
    applyView();
    wake(0.3);
  }

  /** Bildschirmposition in Koordinaten der Simulation. */
  function toSim(clientX: number, clientY: number) {
    const svg = svgRef.current!;
    const rect = svg.getBoundingClientRect();
    const v = view.current;
    return {
      x: (((clientX - rect.left) * WIDTH) / rect.width - v.x) / v.k,
      y: (((clientY - rect.top) * HEIGHT) / rect.height - v.y) / v.k,
    };
  }

  function onPointerDown(event: PointerEvent) {
    if ((event.target as Element).closest('.node')) return;
    pan.current = { x: event.clientX, y: event.clientY };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    const start = pan.current;
    const svg = svgRef.current;
    if (!start || !svg) return;
    autoFit.current = false;
    const rect = svg.getBoundingClientRect();
    const dx = ((event.clientX - start.x) * WIDTH) / rect.width;
    const dy = ((event.clientY - start.y) * HEIGHT) / rect.height;
    pan.current = { x: event.clientX, y: event.clientY };
    const v = view.current;
    view.current = { ...v, x: v.x + dx, y: v.y + dy };
    applyView();
  }

  // Knoten ziehen: Der Knoten folgt dem Zeiger, die Simulation läuft währenddessen weiter, die Nachbarn ziehen mit.
  function onNodeDown(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    event.stopPropagation();
    nodeDrag.current = { id, x: event.clientX, y: event.clientY, moved: false };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }

  function onNodeMove(event: PointerEvent) {
    const d = nodeDrag.current;
    if (!d) return;
    if (!d.moved && Math.hypot(event.clientX - d.x, event.clientY - d.y) < DRAG_THRESHOLD) return;
    d.moved = true;
    const p = toSim(event.clientX, event.clientY);
    sim.pin(d.id, p.x, p.y);
    alphaTarget.current = 0.3;
    wake(0.3);
    if (reducedMotion()) paint();
  }

  function onNodeUp() {
    const d = nodeDrag.current;
    nodeDrag.current = null;
    if (!d) return;
    sim.release(d.id);
    alphaTarget.current = 0;
    if (d.moved) {
      // Auf das Ziehen folgt ein Klick-Ereignis, das keine Auswahl sein soll.
      suppressClick.current = true;
      setTimeout(() => (suppressClick.current = false), 0);
      wake(0.3);
    }
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

  function changeForce(key: keyof SimulationParams, value: number) {
    const next = { ...forces, [key]: value };
    setForces(next);
    sim.setParams({ [key]: value });
    wake(0.6);
  }

  function resetForces() {
    setForces(DEFAULT_FORCES);
    sim.setParams(DEFAULT_FORCES);
    wake(0.8);
  }

  const dimmed = (id: string) => {
    if (q !== '') return !matching.has(id);
    if (focusId) return id !== focusId && !neighborIds.has(id);
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
        {schoolKind && !hidden.has('schule') && (
          <SchoolControls kind={schoolKind} schools={schools} group={group} onGroup={setGroup} filter={filter} onFilter={setFilter} />
        )}
        <div class="row">
          <button type="button" aria-label="Vergrößern" onClick={() => zoomBy(1.3)}>+</button>
          <button type="button" aria-label="Verkleinern" onClick={() => zoomBy(1 / 1.3)}>−</button>
          <button type="button" onClick={resetView}>Ansicht zurücksetzen</button>
        </div>
        <details class="graph-forces">
          <summary>Kräfte</summary>
          <div class="forces">
            {FORCE_SLIDERS.map((s) => (
              <label key={s.key}>
                {s.label}
                <input
                  type="range"
                  min={s.min}
                  max={s.max}
                  step={s.step}
                  value={forces[s.key]}
                  onInput={(e) => changeForce(s.key, Number(e.currentTarget.value))}
                />
              </label>
            ))}
            <button type="button" class="link" onClick={resetForces}>Voreinstellung</button>
          </div>
        </details>
      </div>
      {error && <p class="error" role="alert">{error}</p>}

      <div class="graph-body">
        <svg
          ref={svgRef}
          viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
          role="group"
          aria-label="Graph der Einträge. Knoten ziehen, Hintergrund ziehen verschiebt, Mausrad zoomt."
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={() => (pan.current = null)}
          onPointerCancel={() => (pan.current = null)}
          onClick={(e) => {
            if (!(e.target as Element).closest('.node')) setSelected(null);
          }}
        >
          <g ref={rootRef}>
            {[...boxes].map(([label, b]) => {
              const a = boxAttrs(b);
              return (
                <g key={label} class="group-box" data-testid="graph-group" data-group={label}>
                  <rect x={a.x} y={a.y} width={a.width} height={a.height} rx={22} />
                  <text x={a.tx} y={a.ty}>{`${label} · ${b.count}`}</text>
                </g>
              );
            })}
            {graph.edges.map((e) => {
              const a = sim.position(e.source);
              const b = sim.position(e.target);
              if (!a || !b) return null;
              const active = focusId && (e.source === focusId || e.target === focusId);
              return (
                <line
                  key={`${e.source}-${e.target}`}
                  data-testid="graph-edge"
                  data-source={e.source}
                  data-target={e.target}
                  class={`edge${active ? ' active' : ''}${dimmed(e.source) || dimmed(e.target) ? ' dimmed' : ''}`}
                  x1={a.x}
                  y1={a.y}
                  x2={b.x}
                  y2={b.y}
                />
              );
            })}
            {graph.nodes.map((n) => {
              const p = sim.position(n.id);
              if (!p) return null;
              const classes = ['node', `kind-${(kindIndex.get(n.kind) ?? 0) % 8}`];
              if (n.id === selected) classes.push('selected');
              if (matching.has(n.id)) classes.push('match');
              if (dimmed(n.id)) classes.push('dimmed');
              const nonstate = n.kind === 'schule' && byId.get(n.id)?.fields.traegerschaft === 'Nichtstaatlich';
              if (nonstate) classes.push('nonstate');
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
                  aria-label={`${kindLabel(n.kind)}: ${n.label}${nonstate ? ' (nichtstaatlich)' : ''}`}
                  aria-pressed={n.id === selected}
                  onPointerDown={(e) => onNodeDown(e, n.id)}
                  onPointerMove={onNodeMove}
                  onPointerUp={onNodeUp}
                  onPointerCancel={onNodeUp}
                  onPointerEnter={() => !nodeDrag.current && setHovered(n.id)}
                  onPointerLeave={() => !nodeDrag.current && setHovered(null)}
                  onFocus={() => setHovered(n.id)}
                  onBlur={() => setHovered(null)}
                  onClick={() => {
                    if (!suppressClick.current) setSelected(n.id);
                  }}
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
            <p class="muted">
              {graph.nodes.length} Einträge, {graph.edges.length} Verbindungen. Wähle einen Knoten, um seine Verbindungen zu sehen, oder ziehe ihn, um das Netz zu bewegen.
              {boxes.size > 0 && ' Gestrichelte Rahmen fassen Schulen einer Gruppe zusammen, Schulen mit gestricheltem Ring sind nichtstaatlich.'}
            </p>
          )}
          {selectedNode && (
            <>
              <h3>{selectedNode.label}</h3>
              <p class="muted">
                {[kindLabel(selectedNode.kind), ...(selectedNode.kind === 'schule' && byId.has(selectedNode.id)
                  ? (['schulart', 'ort', 'traegerschaft'] as const).map((key) => valueOf(byId.get(selectedNode.id)!, key))
                  : [])].filter(Boolean).join(' · ')}
              </p>
              <button type="button" class="primary" onClick={() => onOpen(selectedNode.kind, selectedNode.id)}>Bearbeiten</button>
              <h4>Verbunden mit</h4>
              {neighbors(graph, selectedNode.id).length === 0 && <p class="muted">Keine Verbindungen.</p>}
              <ul>
                {neighbors(graph, selectedNode.id).map((id) => {
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
