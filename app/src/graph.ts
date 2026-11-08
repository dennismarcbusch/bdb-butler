// Graph der Einträge: Knoten sind Einträge, Kanten sind Verweise zwischen ihnen. Die Verweise ergeben sich
// aus den Typdefinitionen (Felder vom Typ `ref` und `ref_list`), neue Typen erscheinen damit automatisch.

import { wikilinks } from './markdown';
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

/**
 * Index von Titeln (klein geschrieben) auf Eintrags-IDs für Wikilinks. Ist ein Titel mehrdeutig (mehrere
 * Einträge heißen gleich), steht `null` im Index: Der Link bleibt dann unaufgelöst, statt irgendeinen Eintrag zu
 * treffen.
 */
export function titleIndex(records: RecordView[], kinds: KindDef[]): Map<string, string | null> {
  const byKind = new Map(kinds.map((k) => [k.kind, k]));
  const index = new Map<string, string | null>();
  for (const record of records) {
    const kind = byKind.get(record.kind);
    const title = kind ? record.fields[kind.title_field] : undefined;
    if (typeof title !== 'string' || title.trim() === '') continue;
    const key = title.trim().toLowerCase();
    index.set(key, index.has(key) ? null : record.id);
  }
  return index;
}

function targetsOf(
  record: RecordView,
  kind: KindDef,
  index: Map<string, string | null>,
): { field: string; id: string }[] {
  const out: { field: string; id: string }[] = [];
  for (const field of kind.fields) {
    const value = record.fields[field.name];
    if (field.type === 'ref' || field.type === 'ref_list') {
      const ids = Array.isArray(value) ? value : [value];
      for (const id of ids) {
        if (typeof id === 'string') out.push({ field: field.name, id });
      }
    } else if (field.type === 'markdown' && typeof value === 'string') {
      // Wikilinks [[Titel]] verweisen auf Einträge mit diesem Titel.
      for (const title of wikilinks(value)) {
        const id = index.get(title.trim().toLowerCase());
        if (id) out.push({ field: field.name, id });
      }
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
  const index = titleIndex(records, kinds);
  const nodes: GraphNode[] = records.map((r) => ({ id: r.id, kind: r.kind, label: labelOf(r, byKind.get(r.kind)) }));

  const seen = new Set<string>();
  const edges: GraphEdge[] = [];
  for (const record of records) {
    const kind = byKind.get(record.kind);
    if (!kind) continue;
    for (const { id } of targetsOf(record, kind, index)) {
      if (id === record.id || !known.has(id)) continue;
      const key = record.id < id ? `${record.id}|${id}` : `${id}|${record.id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push({ source: record.id, target: id });
    }
  }
  return { nodes, edges };
}

/**
 * Entfernt die Einträge in `removed` aus `records` und mit ihnen alle, deren sämtliche Verbindungen zu entfernten
 * Einträgen führen (zum Beispiel Ansprechpartner einer ausgefilterten Schule). Einträge ohne Verbindungen und solche
 * mit mindestens einer verbleibenden Verbindung bleiben.
 */
export function dropWithOrphans(records: RecordView[], kinds: KindDef[], removed: Set<string>): RecordView[] {
  if (removed.size === 0) return records;
  const full = buildGraph(records, kinds);
  const near = new Map<string, string[]>();
  for (const e of full.edges) {
    (near.get(e.source) ?? near.set(e.source, []).get(e.source)!).push(e.target);
    (near.get(e.target) ?? near.set(e.target, []).get(e.target)!).push(e.source);
  }
  return records.filter((r) => {
    if (removed.has(r.id)) return false;
    const ids = near.get(r.id) ?? [];
    return ids.length === 0 || ids.some((id) => !removed.has(id));
  });
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
  const index = titleIndex(records, kinds);
  const self = byId.get(id);
  const out: Related[] = [];

  const label = (kind: KindDef, field: string) => kind.fields.find((f) => f.name === field)?.label ?? field;

  if (self) {
    const kind = byKind.get(self.kind);
    if (kind) {
      for (const t of targetsOf(self, kind, index)) {
        const other = byId.get(t.id);
        if (other && other.id !== id) out.push({ record: other, direction: 'out', fieldLabel: label(kind, t.field) });
      }
    }
  }
  for (const record of records) {
    if (record.id === id) continue;
    const kind = byKind.get(record.kind);
    if (!kind) continue;
    for (const t of targetsOf(record, kind, index)) {
      if (t.id === id) out.push({ record, direction: 'in', fieldLabel: label(kind, t.field) });
    }
  }
  return out;
}


export { Simulation, fitView, layout, temperatureFor } from './simulation';
export type { LayoutOptions, SimulationParams } from './simulation';
