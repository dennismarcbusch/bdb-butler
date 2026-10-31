// Graph der Einträge: Knoten sind Einträge, Kanten sind Verweise zwischen ihnen. Die Verweise ergeben sich
// aus den Typdefinitionen (Felder vom Typ `ref` und `ref_list`), neue Typen erscheinen damit automatisch.

import type { KindDef, RecordView } from './types';

export interface GraphNode {
  id: string;
  kind: string;
  label: string;
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface Point {
  x: number;
  y: number;
}

function targetsOf(record: RecordView, kind: KindDef): { field: string; id: string }[] {
  const out: { field: string; id: string }[] = [];
  for (const field of kind.fields) {
    if (field.type !== 'ref' && field.type !== 'ref_list') continue;
    const value = record.fields[field.name];
    const ids = Array.isArray(value) ? value : [value];
    for (const id of ids) {
      if (typeof id === 'string') out.push({ field: field.name, id });
    }
  }
  return out;
}

function labelOf(record: RecordView, kind: KindDef | undefined): string {
  const title = kind ? record.fields[kind.title_field] : undefined;
  return typeof title === 'string' && title !== '' ? title : '(ohne Titel)';
}

/** Baut den Graphen. Verweise auf fehlende Einträge, auf sich selbst und doppelte Verbindungen entfallen. */
export function buildGraph(records: RecordView[], kinds: KindDef[]): Graph {
  const byKind = new Map(kinds.map((k) => [k.kind, k]));
  const known = new Set(records.map((r) => r.id));
  const nodes: GraphNode[] = records.map((r) => ({ id: r.id, kind: r.kind, label: labelOf(r, byKind.get(r.kind)) }));

  const seen = new Set<string>();
  const edges: GraphEdge[] = [];
  for (const record of records) {
    const kind = byKind.get(record.kind);
    if (!kind) continue;
    for (const { id } of targetsOf(record, kind)) {
      if (id === record.id || !known.has(id)) continue;
      const key = record.id < id ? `${record.id}|${id}` : `${id}|${record.id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push({ source: record.id, target: id });
    }
  }
  return { nodes, edges };
}

/** IDs der direkt verbundenen Knoten. */
export function neighbors(graph: Graph, id: string): string[] {
  const out = new Set<string>();
  for (const e of graph.edges) {
    if (e.source === id) out.add(e.target);
    else if (e.target === id) out.add(e.source);
  }
  return [...out];
}

export interface Related {
  record: RecordView;
  /// `out`: dieser Eintrag verweist auf den anderen, `in`: der andere verweist auf diesen.
  direction: 'out' | 'in';
  fieldLabel: string;
}

/** Einträge, die mit dem Eintrag `id` über Verweise verbunden sind (in beide Richtungen). */
export function related(records: RecordView[], kinds: KindDef[], id: string): Related[] {
  const byKind = new Map(kinds.map((k) => [k.kind, k]));
  const byId = new Map(records.map((r) => [r.id, r]));
  const self = byId.get(id);
  const out: Related[] = [];

  const label = (kind: KindDef, field: string) => kind.fields.find((f) => f.name === field)?.label ?? field;

  if (self) {
    const kind = byKind.get(self.kind);
    if (kind) {
      for (const t of targetsOf(self, kind)) {
        const other = byId.get(t.id);
        if (other && other.id !== id) out.push({ record: other, direction: 'out', fieldLabel: label(kind, t.field) });
      }
    }
  }
  for (const record of records) {
    if (record.id === id) continue;
    const kind = byKind.get(record.kind);
    if (!kind) continue;
    for (const t of targetsOf(record, kind)) {
      if (t.id === id) out.push({ record, direction: 'in', fieldLabel: label(kind, t.field) });
    }
  }
  return out;
}

export interface LayoutOptions {
  width?: number;
  height?: number;
  iterations?: number;
  padding?: number;
  /// Wunschabstand verbundener Knoten in Einheiten der Zeichenfläche (Standard 110, passend zur Beschriftung).
  spacing?: number;
  /// Stärke der Schwerkraft zur Mitte (Standard 0.3). Zu schwach: unverbundene Knoten wandern an den Rand und
  /// verbundene rücken eng zusammen. Zu stark: getrennte Gruppen vermischen sich.
  gravity?: number;
  /// Größte Vergrößerung beim Einpassen. Kleine Graphen werden nicht über die Fläche gestreckt.
  maxScale?: number;
}

/**
 * Kraftbasiertes Layout (Fruchterman und Reingold): Knoten stoßen sich ab, Kanten ziehen zusammen, eine
 * schwache Schwerkraft hält getrennte Gruppen beisammen. Die Startpositionen sind deterministisch, dasselbe
 * Graph-Eingangssignal ergibt dasselbe Bild. Die Ergebnisse liegen innerhalb von `padding` bis
 * `width - padding` beziehungsweise `height - padding`.
 */
export function layout(graph: Graph, options: LayoutOptions = {}): Map<string, Point> {
  const width = options.width ?? 1000;
  const height = options.height ?? 700;
  const iterations = options.iterations ?? 300;
  const padding = options.padding ?? 50;
  const maxScale = options.maxScale ?? 1.5;
  const gravity = options.gravity ?? 0.3;
  const spacing = options.spacing ?? 110;
  const n = graph.nodes.length;
  const result = new Map<string, Point>();
  if (n === 0) return result;
  if (n === 1) {
    result.set(graph.nodes[0]!.id, { x: width / 2, y: height / 2 });
    return result;
  }

  const index = new Map(graph.nodes.map((node, i) => [node.id, i]));
  const x = new Float64Array(n);
  const y = new Float64Array(n);
  const golden = 2.399963229728653;
  for (let i = 0; i < n; i++) {
    const r = 40 * Math.sqrt(i + 1);
    x[i] = width / 2 + r * Math.cos(i * golden);
    y[i] = height / 2 + r * Math.sin(i * golden);
  }
  const edges = graph.edges
    .map((e) => [index.get(e.source), index.get(e.target)] as const)
    .filter((e): e is readonly [number, number] => e[0] !== undefined && e[1] !== undefined);

  const k = spacing * (1 + 0.5 * Math.log10(n));
  const dx = new Float64Array(n);
  const dy = new Float64Array(n);

  for (let it = 0; it < iterations; it++) {
    const temperature = (width / 8) * (1 - it / iterations) + 0.5;
    dx.fill(0);
    dy.fill(0);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let ex = x[i]! - x[j]!;
        let ey = y[i]! - y[j]!;
        let d = Math.hypot(ex, ey);
        if (d < 0.01) {
          // Genau übereinander: deterministisch auseinanderschieben.
          ex = 0.01 * ((i + j) % 2 === 0 ? 1 : -1);
          ey = 0.01;
          d = Math.hypot(ex, ey);
        }
        const force = (k * k) / d;
        const fx = (ex / d) * force;
        const fy = (ey / d) * force;
        dx[i]! += fx;
        dy[i]! += fy;
        dx[j]! -= fx;
        dy[j]! -= fy;
      }
    }
    for (const [a, b] of edges) {
      const ex = x[a]! - x[b]!;
      const ey = y[a]! - y[b]!;
      const d = Math.max(Math.hypot(ex, ey), 0.01);
      const force = (d * d) / k;
      const fx = (ex / d) * force;
      const fy = (ey / d) * force;
      dx[a]! -= fx;
      dy[a]! -= fy;
      dx[b]! += fx;
      dy[b]! += fy;
    }
    for (let i = 0; i < n; i++) {
      dx[i]! += (width / 2 - x[i]!) * gravity;
      dy[i]! += (height / 2 - y[i]!) * gravity;
      const len = Math.hypot(dx[i]!, dy[i]!);
      if (len > 0) {
        const move = Math.min(len, temperature);
        x[i]! += (dx[i]! / len) * move;
        y[i]! += (dy[i]! / len) * move;
      }
    }
  }

  // In den sichtbaren Bereich einpassen, Seitenverhältnis erhalten.
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (let i = 0; i < n; i++) {
    minX = Math.min(minX, x[i]!); maxX = Math.max(maxX, x[i]!);
    minY = Math.min(minY, y[i]!); maxY = Math.max(maxY, y[i]!);
  }
  const spanX = Math.max(maxX - minX, 1);
  const spanY = Math.max(maxY - minY, 1);
  const scale = Math.min((width - 2 * padding) / spanX, (height - 2 * padding) / spanY, maxScale);
  const offsetX = (width - spanX * scale) / 2;
  const offsetY = (height - spanY * scale) / 2;
  graph.nodes.forEach((node, i) => {
    result.set(node.id, { x: offsetX + (x[i]! - minX) * scale, y: offsetY + (y[i]! - minY) * scale });
  });
  return result;
}
