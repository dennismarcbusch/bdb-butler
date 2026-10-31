// Kleiner Markdown-Parser für Protokolle. Er erzeugt einen Syntaxbaum und kein HTML. Der Baum wird mit Preact
// in Knoten umgesetzt, dadurch kann keine Eingabe HTML oder Skripte einschleusen.
//
// Unterstützt: Überschriften, Absätze (einfache Zeilenumbrüche bleiben erhalten), Listen (auch verschachtelt,
// nummeriert, Aufgabenlisten `- [ ]` und `- [x]`), Zitate, Codeblöcke, Trennlinien sowie **fett**, *kursiv*,
// `Code`, [Text](Adresse) und Wikilinks [[Titel]] oder [[Titel|Anzeigetext]].

export type Inline =
  | { t: 'text'; v: string }
  | { t: 'br' }
  | { t: 'code'; v: string }
  | { t: 'strong'; c: Inline[] }
  | { t: 'em'; c: Inline[] }
  | { t: 'link'; href: string; c: Inline[] }
  | { t: 'wikilink'; title: string; label: string };

export interface ListItem {
  /// `null`: normaler Eintrag, sonst Aufgabe mit Erledigt-Kennzeichen.
  task: boolean | null;
  c: Inline[];
  /// Index der Zeile im Quelltext (für Aufgaben, die sich ändern lassen).
  line: number;
  children: Block[];
}

export type Block =
  | { t: 'heading'; level: number; c: Inline[] }
  | { t: 'paragraph'; c: Inline[] }
  | { t: 'list'; ordered: boolean; items: ListItem[] }
  | { t: 'quote'; c: Block[] }
  | { t: 'code'; v: string; lang: string }
  | { t: 'hr' };

const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const HR = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/;
const FENCE = /^\s{0,3}(```+|~~~+)\s*([\w+-]*)\s*$/;
const LIST_ITEM = /^(\s*)([-*+]|\d{1,9}[.)])\s+(?:\[( |x|X)\]\s+)?(.*)$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;

function normalize(source: string): string[] {
  return source.replace(/\r\n?/g, '\n').split('\n');
}

const SAFE_PROTOCOLS = /^(https?:|mailto:)/i;

function isSafeUrl(url: string): boolean {
  return SAFE_PROTOCOLS.test(url.trim());
}

/** Zerlegt Text in Inline-Elemente. */
export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  let buffer = '';
  const flush = () => {
    if (buffer !== '') out.push({ t: 'text', v: buffer });
    buffer = '';
  };
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);
    const ch = text[i]!;

    if (ch === '\\' && i + 1 < text.length && /[\\`*_{}[\]()#+\-.!|<>]/.test(text[i + 1]!)) {
      buffer += text[i + 1];
      i += 2;
      continue;
    }
    if (ch === '`') {
      const m = /^(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/.exec(rest);
      if (m) {
        flush();
        out.push({ t: 'code', v: m[2]!.trim() });
        i += m[0].length;
        continue;
      }
    }
    if (ch === '[' && rest.startsWith('[[')) {
      const m = /^\[\[([^\]\n|]+?)(?:\|([^\]\n]+?))?\]\]/.exec(rest);
      if (m) {
        flush();
        const title = m[1]!.trim();
        out.push({ t: 'wikilink', title, label: (m[2] ?? title).trim() });
        i += m[0].length;
        continue;
      }
    }
    if (ch === '[') {
      // Die Adresse darf ausgeglichene Klammern enthalten, zum Beispiel `https://de.wikipedia.org/wiki/X_(Y)`.
      const m = /^\[([^\]\n]+)\]\(([^\s()]*(?:\([^\s()]*\)[^\s()]*)*)\)/.exec(rest);
      if (m) {
        flush();
        if (isSafeUrl(m[2]!)) out.push({ t: 'link', href: m[2]!, c: parseInline(m[1]!) });
        else out.push({ t: 'text', v: m[1]! });
        i += m[0].length;
        continue;
      }
    }
    if (ch === '*' && rest.startsWith('**')) {
      // Das schließende Paar darf nicht von einem weiteren Stern gefolgt werden, damit `***` richtig schließt.
      const m = /^\*\*(?=\S)([\s\S]*?\S)\*\*(?!\*)/.exec(rest);
      if (m) {
        flush();
        out.push({ t: 'strong', c: parseInline(m[1]!) });
        i += m[0].length;
        continue;
      }
    }
    if (ch === '*' || ch === '_') {
      // Unterstriche nur an Wortgrenzen, damit snake_case nicht kursiv wird.
      const prev = i > 0 ? text[i - 1]! : ' ';
      const boundaryOk = ch === '*' || !/[\p{L}\p{N}]/u.test(prev);
      const m = ch === '*' ? /^\*(?=\S)([^*\n]*?\S)\*(?!\*)/.exec(rest) : /^_(?=\S)([^_\n]*?\S)_(?![\p{L}\p{N}_])/u.exec(rest);
      if (m && boundaryOk) {
        flush();
        out.push({ t: 'em', c: parseInline(m[1]!) });
        i += m[0].length;
        continue;
      }
    }
    buffer += ch;
    i += 1;
  }
  flush();
  return out;
}

/** Inline-Elemente eines mehrzeiligen Absatzes: Zeilenumbrüche bleiben als `br` erhalten. */
function parseLines(lines: string[]): Inline[] {
  const out: Inline[] = [];
  lines.forEach((line, index) => {
    if (index > 0) out.push({ t: 'br' });
    out.push(...parseInline(line.trim()));
  });
  return out;
}

function indentOf(whitespace: string): number {
  return whitespace.replace(/\t/g, '    ').length;
}

interface ParsedItem {
  indent: number;
  ordered: boolean;
  task: boolean | null;
  text: string;
  line: number;
}

function buildList(items: ParsedItem[], start: number, indent: number): { block: Block; next: number } {
  const ordered = items[start]!.ordered;
  const list: ListItem[] = [];
  let i = start;
  while (i < items.length) {
    const item = items[i]!;
    if (item.indent < indent) break;
    if (item.indent > indent) {
      // Tiefer eingerückt: gehört als Unterliste zum letzten Eintrag.
      const parent = list[list.length - 1];
      if (!parent) break;
      const nested = buildList(items, i, item.indent);
      parent.children.push(nested.block);
      i = nested.next;
      continue;
    }
    if (item.ordered !== ordered && list.length > 0) break;
    list.push({ task: item.task, c: parseInline(item.text), line: item.line, children: [] });
    i += 1;
  }
  return { block: { t: 'list', ordered, items: list }, next: i };
}

function parseBlocks(lines: string[], lineOffset = 0): Block[] {
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    if (line.trim() === '') {
      i += 1;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const marker = fence[1]!;
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i]!.trim().startsWith(marker)) {
        body.push(lines[i]!);
        i += 1;
      }
      i += 1; // schließende Zeile (oder Ende)
      blocks.push({ t: 'code', v: body.join('\n'), lang: fence[2] ?? '' });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({ t: 'heading', level: heading[1]!.length, c: parseInline(heading[2]!) });
      i += 1;
      continue;
    }

    if (HR.test(line)) {
      blocks.push({ t: 'hr' });
      i += 1;
      continue;
    }

    if (QUOTE.test(line)) {
      const inner: string[] = [];
      const first = i;
      while (i < lines.length && QUOTE.test(lines[i]!)) {
        inner.push(QUOTE.exec(lines[i]!)![1]!);
        i += 1;
      }
      blocks.push({ t: 'quote', c: parseBlocks(inner, lineOffset + first) });
      continue;
    }

    if (LIST_ITEM.test(line)) {
      const items: ParsedItem[] = [];
      while (i < lines.length) {
        const m = LIST_ITEM.exec(lines[i]!);
        if (!m || HR.test(lines[i]!)) break;
        items.push({
          indent: indentOf(m[1]!),
          ordered: /\d/.test(m[2]!),
          task: m[3] === undefined ? null : m[3] !== ' ',
          text: m[4]!,
          line: lineOffset + i,
        });
        i += 1;
      }
      let pos = 0;
      while (pos < items.length) {
        const built = buildList(items, pos, items[pos]!.indent);
        blocks.push(built.block);
        pos = built.next;
      }
      continue;
    }

    // Absatz: bis zur nächsten Leerzeile oder zum nächsten Blockbeginn.
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i]!.trim() !== '' &&
      !HEADING.test(lines[i]!) &&
      !FENCE.test(lines[i]!) &&
      !QUOTE.test(lines[i]!) &&
      !HR.test(lines[i]!) &&
      !LIST_ITEM.test(lines[i]!)
    ) {
      para.push(lines[i]!);
      i += 1;
    }
    blocks.push({ t: 'paragraph', c: parseLines(para) });
  }
  return blocks;
}

export function parseMarkdown(source: string): Block[] {
  return parseBlocks(normalize(source));
}

function inlineText(inlines: Inline[]): string {
  return inlines
    .map((n) => {
      switch (n.t) {
        case 'text':
        case 'code':
          return n.v;
        case 'br':
          return ' ';
        case 'wikilink':
          return n.label;
        default:
          return inlineText(n.c);
      }
    })
    .join('');
}

function collect(blocks: Block[], visit: (inline: Inline[]) => void) {
  for (const block of blocks) {
    switch (block.t) {
      case 'heading':
      case 'paragraph':
        visit(block.c);
        break;
      case 'quote':
        collect(block.c, visit);
        break;
      case 'list':
        for (const item of block.items) {
          visit(item.c);
          collect(item.children, visit);
        }
        break;
      default:
        break;
    }
  }
}

function walkInline(inlines: Inline[], visit: (n: Inline) => void) {
  for (const n of inlines) {
    visit(n);
    if (n.t === 'strong' || n.t === 'em' || n.t === 'link') walkInline(n.c, visit);
  }
}

/** Alle Wikilink-Titel im Text, ohne Wiederholungen. Code (inline und Blöcke) zählt nicht. */
export function wikilinks(source: string): string[] {
  const titles: string[] = [];
  collect(parseMarkdown(source), (inline) =>
    walkInline(inline, (n) => {
      if (n.t === 'wikilink' && !titles.includes(n.title)) titles.push(n.title);
    }),
  );
  return titles;
}

export interface Task {
  /// Zeile im Quelltext.
  line: number;
  done: boolean;
  text: string;
}

/** Aufgaben (`- [ ] …`) im Text mit Zeilennummer. Aufgaben in Codeblöcken zählen nicht. */
export function extractTasks(source: string): Task[] {
  const tasks: Task[] = [];
  const visitList = (blocks: Block[]) => {
    for (const block of blocks) {
      if (block.t === 'list') {
        for (const item of block.items) {
          if (item.task !== null) tasks.push({ line: item.line, done: item.task, text: inlineText(item.c).trim() });
          visitList(item.children);
        }
      } else if (block.t === 'quote') {
        visitList(block.c);
      }
    }
  };
  visitList(parseMarkdown(source));
  return tasks.sort((a, b) => a.line - b.line);
}

/** Setzt oder löscht das Erledigt-Kennzeichen einer Aufgabe in der angegebenen Zeile. */
export function setTaskDone(source: string, line: number, done: boolean): string {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const current = lines[line];
  if (current === undefined) return source;
  lines[line] = current.replace(/\[( |x|X)\]/, done ? '[x]' : '[ ]');
  return lines.join('\n');
}
