import { describe, expect, it } from 'vitest';
import { buildGraph, layout, neighbors, related, type Graph } from './graph';
import type { KindDef, RecordView } from './types';

const kinds: KindDef[] = [
  {
    kind: 'schule', label: 'Schule', label_plural: 'Schulen', title_field: 'name', personal: false,
    fields: [{ name: 'name', label: 'Name', type: 'text', required: true }],
  },
  {
    kind: 'person', label: 'Person', label_plural: 'Personen', title_field: 'name', personal: true,
    fields: [
      { name: 'name', label: 'Name', type: 'text', required: true },
      { name: 'schule', label: 'Schule', type: 'ref', kind: 'schule', required: false },
      { name: 'kontakte', label: 'Verknüpft mit', type: 'ref_list', kind: 'person', required: false },
    ],
  },
];

const rec = (id: string, kind: string, fields: RecordView['fields']): RecordView => ({ id, kind, fields });

const records: RecordView[] = [
  rec('s1', 'schule', { name: 'Grundschule' }),
  rec('s2', 'schule', { name: 'Gymnasium' }),
  rec('p1', 'person', { name: 'Weber', schule: 's1', kontakte: ['p2'] }),
  rec('p2', 'person', { name: 'Bauer', schule: 's1', kontakte: ['p1'] }),
  rec('p3', 'person', { name: 'Klein', schule: 's2' }),
  rec('p4', 'person', { name: 'Allein' }),
];

describe('buildGraph', () => {
  it('macht aus Einträgen Knoten und aus Verweisen Kanten', () => {
    const g = buildGraph(records, kinds);
    expect(g.nodes.map((n) => n.id)).toEqual(['s1', 's2', 'p1', 'p2', 'p3', 'p4']);
    expect(g.nodes[2]).toEqual({ id: 'p1', kind: 'person', label: 'Weber' });
    // p1-s1, p1-p2, p2-s1, p3-s2; p2->p1 ist dieselbe Verbindung wie p1->p2.
    expect(g.edges).toHaveLength(4);
    expect(g.edges.some((e) => [e.source, e.target].sort().join() === 'p1,p2')).toBe(true);
  });

  it('ignoriert Verweise auf fehlende Einträge und auf sich selbst', () => {
    const g = buildGraph([
      rec('a', 'person', { name: 'A', schule: 'gibt-es-nicht', kontakte: ['a'] }),
    ], kinds);
    expect(g.nodes).toHaveLength(1);
    expect(g.edges).toEqual([]);
  });

  it('kommt mit unbekannten Typen und leeren Listen zurecht', () => {
    const g = buildGraph([rec('x', 'unbekannt', {}), rec('p', 'person', { name: 'P', kontakte: [] })], kinds);
    expect(g.nodes.map((n) => n.label)).toEqual(['(ohne Titel)', 'P']);
    expect(g.edges).toEqual([]);
    expect(buildGraph([], kinds)).toEqual({ nodes: [], edges: [] });
  });

  it('findet Nachbarn', () => {
    const g = buildGraph(records, kinds);
    expect(neighbors(g, 's1').sort()).toEqual(['p1', 'p2']);
    expect(neighbors(g, 'p4')).toEqual([]);
  });
});

describe('related', () => {
  it('liefert Verweise in beide Richtungen mit Feldbezeichnung', () => {
    const rel = related(records, kinds, 's1');
    expect(rel.map((r) => [r.record.id, r.direction, r.fieldLabel])).toEqual([
      ['p1', 'in', 'Schule'],
      ['p2', 'in', 'Schule'],
    ]);
    const forP1 = related(records, kinds, 'p1');
    expect(forP1.filter((r) => r.direction === 'out').map((r) => r.record.id).sort()).toEqual(['p2', 's1']);
    expect(forP1.filter((r) => r.direction === 'in').map((r) => r.record.id)).toEqual(['p2']);
    expect(related(records, kinds, 'p4')).toEqual([]);
    expect(related(records, kinds, 'gibt-es-nicht')).toEqual([]);
  });
});

function pathGraph(n: number): Graph {
  const nodes = Array.from({ length: n }, (_, i) => ({ id: `n${i}`, kind: 'x', label: `n${i}` }));
  const edges = nodes.slice(1).map((node, i) => ({ source: `n${i}`, target: node.id }));
  return { nodes, edges };
}

describe('layout', () => {
  it('liefert keine Positionen für einen leeren Graphen und die Mitte für einen einzelnen Knoten', () => {
    expect(layout({ nodes: [], edges: [] }).size).toBe(0);
    const single = layout({ nodes: [{ id: 'a', kind: 'x', label: 'a' }], edges: [] });
    expect(single.get('a')).toEqual({ x: 500, y: 350 });
  });

  it('ist deterministisch', () => {
    const g = buildGraph(records, kinds);
    expect([...layout(g)]).toEqual([...layout(g)]);
  });

  it('hält alle Knoten im sichtbaren Bereich und ohne ungültige Zahlen', () => {
    for (const g of [buildGraph(records, kinds), pathGraph(2), pathGraph(40)]) {
      const pos = layout(g, { width: 800, height: 500, padding: 40 });
      expect(pos.size).toBe(g.nodes.length);
      for (const p of pos.values()) {
        expect(Number.isFinite(p.x) && Number.isFinite(p.y)).toBe(true);
        expect(p.x).toBeGreaterThanOrEqual(40 - 1e-6);
        expect(p.x).toBeLessThanOrEqual(760 + 1e-6);
        expect(p.y).toBeGreaterThanOrEqual(40 - 1e-6);
        expect(p.y).toBeLessThanOrEqual(460 + 1e-6);
      }
    }
  });

  it('legt verbundene Knoten näher zusammen als unverbundene', () => {
    // Zwei getrennte Dreiecke.
    const ids = ['a1', 'a2', 'a3', 'b1', 'b2', 'b3'];
    const g: Graph = {
      nodes: ids.map((id) => ({ id, kind: 'x', label: id })),
      edges: [
        { source: 'a1', target: 'a2' }, { source: 'a2', target: 'a3' }, { source: 'a1', target: 'a3' },
        { source: 'b1', target: 'b2' }, { source: 'b2', target: 'b3' }, { source: 'b1', target: 'b3' },
      ],
    };
    const pos = layout(g);
    const dist = (a: string, b: string) => Math.hypot(pos.get(a)!.x - pos.get(b)!.x, pos.get(a)!.y - pos.get(b)!.y);
    const within = (dist('a1', 'a2') + dist('a2', 'a3') + dist('a1', 'a3') + dist('b1', 'b2') + dist('b2', 'b3') + dist('b1', 'b3')) / 6;
    const between = (dist('a1', 'b1') + dist('a2', 'b2') + dist('a3', 'b3') + dist('a1', 'b3') + dist('a2', 'b1')) / 5;
    expect(within).toBeLessThan(between);
  });

  it('hält verbundene Knoten lesbar auseinander, auch wenn unverbundene danebenstehen', () => {
    // Gemessen: Mit zu schwacher Schwerkraft rücken verbundene Knoten auf ~50 Einheiten zusammen,
    // weil unverbundene an den Rand wandern und das Bild danach verkleinert wird.
    const g: Graph = {
      nodes: ['s1', 'p1', 'p2', 'i1', 'i2', 'i3'].map((id) => ({ id, kind: 'x', label: id })),
      edges: [{ source: 's1', target: 'p1' }, { source: 's1', target: 'p2' }, { source: 'p1', target: 'p2' }],
    };
    const pos = layout(g);
    const d = (a: string, b: string) => Math.hypot(pos.get(a)!.x - pos.get(b)!.x, pos.get(a)!.y - pos.get(b)!.y);
    expect(Math.min(d('s1', 'p1'), d('s1', 'p2'), d('p1', 'p2'))).toBeGreaterThan(80);
  });

  it('streckt kleine Graphen nicht über die ganze Fläche', () => {
    const pos = layout(pathGraph(2));
    const [a, b] = [...pos.values()];
    expect(Math.hypot(a!.x - b!.x, a!.y - b!.y)).toBeLessThan(400);
  });

  it('lässt Knoten nicht übereinanderliegen', () => {
    const g = pathGraph(30);
    const pts = [...layout(g).values()];
    let min = Infinity;
    for (let i = 0; i < pts.length; i++)
      for (let j = i + 1; j < pts.length; j++) min = Math.min(min, Math.hypot(pts[i]!.x - pts[j]!.x, pts[i]!.y - pts[j]!.y));
    expect(min).toBeGreaterThan(8);
  });

  it('kommt mit Knoten zurecht, die ohne Kanten alleine stehen', () => {
    const g: Graph = { nodes: ['a', 'b', 'c'].map((id) => ({ id, kind: 'x', label: id })), edges: [] };
    const pos = layout(g);
    expect(new Set([...pos.values()].map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`)).size).toBe(3);
  });
});
