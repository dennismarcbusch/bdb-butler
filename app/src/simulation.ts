// Kraftsimulation für den Graphen. Läuft schrittweise, damit die Oberfläche das Bild fortlaufend bewegen kann
// (Knoten ziehen, Regler verstellen), und liefert über `layout` auch ein festes Bild.

import type { Graph, Point } from './graph';

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
  /// Gruppe je Knoten-ID (zum Beispiel die Schulart einer Schule). Knoten einer Gruppe werden zu einem Haufen
  /// zusammengezogen, die Haufen sitzen getrennt voneinander. Knoten ohne Gruppe folgen nur ihren Kanten.
  groups?: Map<string, string>;
  /// Stärke, mit der Knoten zur Mitte ihrer Gruppe gezogen werden (Standard 1.5, über 2 schwingt das Bild).
  groupPull?: number;
  /// Faktor für die gegenseitige Abstoßung der Knoten (Standard 1).
  repulsion?: number;
  /// Faktor für die Zugkraft der Kanten (Standard 1).
  linkStrength?: number;
  /// Startpositionen vorhandener Knoten, damit ein geänderter Graph nicht neu aufgebaut wird, sondern aus dem
  /// bisherigen Bild heraus weiterläuft.
  initial?: Map<string, Point>;
}

/** Kräfte, die sich zur Laufzeit ändern lassen (Regler in der Oberfläche). */
export type SimulationParams = Pick<LayoutOptions, 'spacing' | 'gravity' | 'groupPull' | 'repulsion' | 'linkStrength'>;

const GOLDEN = 2.399963229728653;

/**
 * Schrittweise Kraftsimulation (Fruchterman und Reingold): Knoten stoßen sich ab, Kanten ziehen zusammen, eine
 * schwache Schwerkraft hält getrennte Gruppen beisammen. Jeder `step` bewegt die Knoten höchstens um die
 * übergebene „Temperatur“. Die Startpositionen sind deterministisch.
 */
export class Simulation {
  readonly ids: string[];
  readonly x: Float64Array;
  readonly y: Float64Array;
  private readonly index: Map<string, number>;
  private readonly edges: (readonly [number, number])[];
  private readonly nodeGroup: number[];
  private readonly centerX: Float64Array;
  private readonly centerY: Float64Array;
  private readonly pinned: Uint8Array;
  private readonly dx: Float64Array;
  private readonly dy: Float64Array;
  private readonly width: number;
  private readonly height: number;
  private readonly params: Required<SimulationParams>;
  /// Größe der größten Gruppe, 0 ohne Gruppen.
  private maxGroup = 0;

  constructor(graph: Graph, options: LayoutOptions = {}) {
    this.width = options.width ?? 1000;
    this.height = options.height ?? 700;
    this.params = {
      spacing: options.spacing ?? 110,
      gravity: options.gravity ?? 0.3,
      groupPull: options.groupPull ?? 1.5,
      repulsion: options.repulsion ?? 1,
      linkStrength: options.linkStrength ?? 1,
    };
    const n = graph.nodes.length;
    this.ids = graph.nodes.map((node) => node.id);
    this.index = new Map(this.ids.map((id, i) => [id, i]));
    this.x = new Float64Array(n);
    this.y = new Float64Array(n);
    this.dx = new Float64Array(n);
    this.dy = new Float64Array(n);
    this.pinned = new Uint8Array(n);
    this.edges = graph.edges
      .map((e) => [this.index.get(e.source), this.index.get(e.target)] as const)
      .filter((e): e is readonly [number, number] => e[0] !== undefined && e[1] !== undefined);

    const cx = this.width / 2;
    const cy = this.height / 2;
    for (let i = 0; i < n; i++) {
      const known = options.initial?.get(this.ids[i]!);
      const r = 40 * Math.sqrt(i + 1);
      this.x[i] = known ? known.x : cx + r * Math.cos(i * GOLDEN);
      this.y[i] = known ? known.y : cy + r * Math.sin(i * GOLDEN);
    }

    // Gruppen: Mittelpunkte gleichmäßig auf einem Kreis, neue Gruppenknoten starten in der Nähe ihres Mittelpunkts.
    const groupIds = [...new Set(graph.nodes.map((node) => options.groups?.get(node.id)).filter((g): g is string => g !== undefined))];
    this.nodeGroup = graph.nodes.map((node) => {
      const g = options.groups?.get(node.id);
      return g === undefined || groupIds.length < 2 ? -1 : groupIds.indexOf(g);
    });
    this.centerX = new Float64Array(groupIds.length);
    this.centerY = new Float64Array(groupIds.length);
    this.maxGroup = 0;
    if (groupIds.length > 1) {
      const sizes = groupIds.map((_, g) => this.nodeGroup.filter((m) => m === g).length);
      this.maxGroup = Math.max(...sizes);
      const k = this.scale();
      // Jede Gruppe bekommt einen Kreisbogen, der so breit ist wie der Haufen, den sie voraussichtlich bildet. So
      // liegen große und kleine Gruppen nebeneinander, ohne sich zu überlappen.
      const radii = sizes.map((s) => k * (0.5 + 0.45 * Math.sqrt(s)));
      const widths = radii.map((r) => 2.2 * r);
      const ring = Math.max(widths.reduce((a, b) => a + b, 0) / (2 * Math.PI), Math.max(...radii) * 1.6);
      let used = 0;
      groupIds.forEach((_, g) => {
        const angle = (used + widths[g]! / 2) / ring - Math.PI / 2;
        used += widths[g]!;
        this.centerX[g] = cx + ring * Math.cos(angle);
        this.centerY[g] = cy + ring * Math.sin(angle);
      });
      const seen = new Array<number>(groupIds.length).fill(0);
      for (let i = 0; i < n; i++) {
        const g = this.nodeGroup[i]!;
        if (g < 0) continue;
        const j = seen[g]!++;
        if (options.initial?.has(this.ids[i]!)) continue;
        const r = 30 * Math.sqrt(j + 1);
        this.x[i] = this.centerX[g]! + r * Math.cos(j * GOLDEN);
        this.y[i] = this.centerY[g]! + r * Math.sin(j * GOLDEN);
      }
    }
  }

  get size(): number {
    return this.ids.length;
  }

  /**
   * Wunschabstand `k` der Knoten. Er wächst mit der Zahl der Knoten, damit große Graphen Luft bekommen. Mit Gruppen
   * zählt nur die größte Gruppe, denn die Haufen sollen kompakt bleiben, nicht mit der Gesamtzahl auseinanderlaufen.
   */
  private scale(): number {
    const basis = this.maxGroup > 0 ? this.maxGroup + 1 : Math.max(this.size, 1);
    return this.params.spacing * (1 + 0.5 * Math.log10(basis));
  }

  /** Ändert Kräfte während des Laufs. */
  setParams(patch: SimulationParams): void {
    for (const [key, value] of Object.entries(patch)) {
      if (value !== undefined) this.params[key as keyof SimulationParams] = value;
    }
  }

  position(id: string): Point | undefined {
    const i = this.index.get(id);
    return i === undefined ? undefined : { x: this.x[i]!, y: this.y[i]! };
  }

  /** Aktuelle Positionen aller Knoten. */
  positions(): Map<string, Point> {
    return new Map(this.ids.map((id, i) => [id, { x: this.x[i]!, y: this.y[i]! }]));
  }

  /** Hält einen Knoten an einer Stelle fest (beim Ziehen). */
  pin(id: string, x: number, y: number): void {
    const i = this.index.get(id);
    if (i === undefined) return;
    this.pinned[i] = 1;
    this.x[i] = x;
    this.y[i] = y;
  }

  release(id: string): void {
    const i = this.index.get(id);
    if (i !== undefined) this.pinned[i] = 0;
  }

  /** Ein Rechenschritt. Gibt die größte Bewegung eines Knotens zurück (klein = Bild ist zur Ruhe gekommen). */
  step(temperature: number): number {
    const { x, y, dx, dy, pinned } = this;
    const n = this.size;
    const { gravity, groupPull, repulsion, linkStrength } = this.params;
    const k = this.scale();
    const reach = k * 2;
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
        // Die Abstoßung hat begrenzte Reichweite. Sonst summiert sich die Wirkung aller fernen Knoten bei vielen
        // Knoten zu einer Kraft, gegen die Schwerkraft und Gruppenkraft nichts ausrichten: Gruppen zerfließen.
        if (d > reach) continue;
        const force = ((k * k) / d) * repulsion;
        const fx = (ex / d) * force;
        const fy = (ey / d) * force;
        dx[i]! += fx;
        dy[i]! += fy;
        dx[j]! -= fx;
        dy[j]! -= fy;
      }
    }
    for (const [a, b] of this.edges) {
      const ex = x[a]! - x[b]!;
      const ey = y[a]! - y[b]!;
      const d = Math.max(Math.hypot(ex, ey), 0.01);
      const force = ((d * d) / k) * linkStrength;
      const fx = (ex / d) * force;
      const fy = (ey / d) * force;
      dx[a]! -= fx;
      dy[a]! -= fy;
      dx[b]! += fx;
      dy[b]! += fy;
    }
    let maxMove = 0;
    for (let i = 0; i < n; i++) {
      const g = this.nodeGroup[i]!;
      if (g >= 0) {
        // Gruppenknoten werden zur Mitte ihrer Gruppe gezogen statt zur Mitte der Fläche.
        dx[i]! += (this.centerX[g]! - x[i]!) * groupPull;
        dy[i]! += (this.centerY[g]! - y[i]!) * groupPull;
      } else {
        dx[i]! += (this.width / 2 - x[i]!) * gravity;
        dy[i]! += (this.height / 2 - y[i]!) * gravity;
      }
      if (pinned[i]) continue;
      const len = Math.hypot(dx[i]!, dy[i]!);
      if (len > 0) {
        const move = Math.min(len, temperature);
        x[i]! += (dx[i]! / len) * move;
        y[i]! += (dy[i]! / len) * move;
        maxMove = Math.max(maxMove, move);
      }
    }
    return maxMove;
  }
}

/**
 * Ansicht (Verschiebung `x`, `y` und Vergrößerung `k`), in der alle Punkte mit Rand `padding` auf die Fläche
 * passen. Das Seitenverhältnis bleibt erhalten, kleine Bilder werden höchstens `maxScale`-fach vergrößert.
 */
export function fitView(
  points: Iterable<Point>,
  width: number,
  height: number,
  padding = 50,
  maxScale = 1.5,
): { x: number; y: number; k: number } {
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (const p of points) {
    minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y);
  }
  if (!Number.isFinite(minX)) return { x: 0, y: 0, k: 1 };
  const spanX = Math.max(maxX - minX, 1);
  const spanY = Math.max(maxY - minY, 1);
  const k = Math.min((width - 2 * padding) / spanX, (height - 2 * padding) / spanY, maxScale);
  return { k, x: (width - spanX * k) / 2 - minX * k, y: (height - spanY * k) / 2 - minY * k };
}

/** Temperatur (größte Bewegung je Schritt) in Abhängigkeit von der Restwärme `alpha` zwischen 0 und 1. */
export function temperatureFor(alpha: number, width = 1000): number {
  return (width / 8) * alpha + 0.5;
}

/**
 * Festes Layout: lässt die Simulation abkühlen und passt das Ergebnis in die Fläche ein. Die Ergebnisse liegen
 * innerhalb von `padding` bis `width - padding` beziehungsweise `height - padding`. Dasselbe Eingangssignal ergibt
 * dasselbe Bild.
 */
export function layout(graph: Graph, options: LayoutOptions = {}): Map<string, Point> {
  const width = options.width ?? 1000;
  const height = options.height ?? 700;
  const iterations = options.iterations ?? 300;
  const padding = options.padding ?? 50;
  const result = new Map<string, Point>();
  const n = graph.nodes.length;
  if (n === 0) return result;
  if (n === 1) {
    result.set(graph.nodes[0]!.id, { x: width / 2, y: height / 2 });
    return result;
  }
  const sim = new Simulation(graph, options);
  for (let it = 0; it < iterations; it++) sim.step(temperatureFor(1 - it / iterations, width));
  const view = fitView(sim.positions().values(), width, height, padding, options.maxScale ?? 1.5);
  for (const [id, p] of sim.positions()) result.set(id, { x: p.x * view.k + view.x, y: p.y * view.k + view.y });
  return result;
}
