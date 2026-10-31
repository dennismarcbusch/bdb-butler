import { describe, expect, it } from 'vitest';
import { extractTasks, parseInline, parseMarkdown, setTaskDone, wikilinks, type Block, type Inline } from './markdown';

const text = (v: string): Inline => ({ t: 'text', v });

describe('Inline', () => {
  it('liest einfachen Text', () => {
    expect(parseInline('Hallo Welt')).toEqual([text('Hallo Welt')]);
    expect(parseInline('')).toEqual([]);
  });

  it('liest fett, kursiv und Code, auch verschachtelt', () => {
    expect(parseInline('a **fett** b *kursiv* c `code`')).toEqual([
      text('a '), { t: 'strong', c: [text('fett')] }, text(' b '), { t: 'em', c: [text('kursiv')] }, text(' c '), { t: 'code', v: 'code' },
    ]);
    expect(parseInline('**fett und *kursiv***')).toEqual([{ t: 'strong', c: [text('fett und '), { t: 'em', c: [text('kursiv')] }] }]);
  });

  it('lässt Formatierung in Code unberührt', () => {
    expect(parseInline('`**kein fett** [[x]]`')).toEqual([{ t: 'code', v: '**kein fett** [[x]]' }]);
  });

  it('macht snake_case und einzelne Zeichen nicht kursiv', () => {
    expect(parseInline('das_ist_snake_case')).toEqual([text('das_ist_snake_case')]);
    expect(parseInline('2 * 3 * 4')).toEqual([text('2 * 3 * 4')]);
    expect(parseInline('_kursiv_ ja')).toEqual([{ t: 'em', c: [text('kursiv')] }, text(' ja')]);
  });

  it('liest Wikilinks mit und ohne Anzeigetext', () => {
    expect(parseInline('Siehe [[Grundschule am See]].')).toEqual([
      text('Siehe '), { t: 'wikilink', title: 'Grundschule am See', label: 'Grundschule am See' }, text('.'),
    ]);
    expect(parseInline('[[Gymnasium Nord|das Gymnasium]]')).toEqual([{ t: 'wikilink', title: 'Gymnasium Nord', label: 'das Gymnasium' }]);
    // Unvollständig: bleibt Text.
    expect(parseInline('[[offen')).toEqual([text('[[offen')]);
  });

  it('erlaubt nur sichere Adressen in Links', () => {
    expect(parseInline('[ok](https://example.org/x)')).toEqual([{ t: 'link', href: 'https://example.org/x', c: [text('ok')] }]);
    expect(parseInline('[mail](mailto:a@b.de)')).toEqual([{ t: 'link', href: 'mailto:a@b.de', c: [text('mail')] }]);
    for (const bad of ['javascript:alert(1)', 'data:text/html,x', 'JaVaScRiPt:alert(1)', 'vbscript:x', '//evil.example', 'file:///etc/passwd']) {
      const result = parseInline(`[klick](${bad})`);
      expect(result).toEqual([text('klick')]);
    }
  });

  it('erlaubt Klammern in Adressen', () => {
    expect(parseInline('[Wiki](https://de.wikipedia.org/wiki/X_(Y)) danach')).toEqual([
      { t: 'link', href: 'https://de.wikipedia.org/wiki/X_(Y)', c: [text('Wiki')] }, text(' danach'),
    ]);
  });

  it('beachtet Maskierung', () => {
    expect(parseInline('\\*kein kursiv\\*')).toEqual([text('*kein kursiv*')]);
    expect(parseInline('\\[[kein link]]')).toEqual([text('[[kein link]]')]);
  });

  it('wertet HTML nicht aus', () => {
    const result = parseInline('<script>alert(1)</script> <img src=x onerror=alert(1)>');
    expect(result).toEqual([text('<script>alert(1)</script> <img src=x onerror=alert(1)>')]);
  });
});

describe('Blöcke', () => {
  const kinds = (blocks: Block[]) => blocks.map((b) => b.t);

  it('liest Überschriften, Absätze und Trennlinien', () => {
    const blocks = parseMarkdown('# Titel\n\nErste Zeile\nZweite Zeile\n\n---\n\n### Klein ###');
    expect(kinds(blocks)).toEqual(['heading', 'paragraph', 'hr', 'heading']);
    expect(blocks[0]).toEqual({ t: 'heading', level: 1, c: [text('Titel')] });
    expect(blocks[1]).toEqual({ t: 'paragraph', c: [text('Erste Zeile'), { t: 'br' }, text('Zweite Zeile')] });
    expect(blocks[3]).toEqual({ t: 'heading', level: 3, c: [text('Klein')] });
  });

  it('liest Listen, nummerierte Listen und Verschachtelung', () => {
    const blocks = parseMarkdown('- eins\n- zwei\n  - zwei a\n  - zwei b\n- drei\n\n1. erst\n2. dann');
    expect(kinds(blocks)).toEqual(['list', 'list']);
    const first = blocks[0] as Extract<Block, { t: 'list' }>;
    expect(first.ordered).toBe(false);
    expect(first.items.map((i) => i.c)).toEqual([[text('eins')], [text('zwei')], [text('drei')]]);
    const nested = first.items[1]!.children[0] as Extract<Block, { t: 'list' }>;
    expect(nested.items.map((i) => i.c)).toEqual([[text('zwei a')], [text('zwei b')]]);
    expect((blocks[1] as Extract<Block, { t: 'list' }>).ordered).toBe(true);
  });

  it('liest Zitate und Codeblöcke', () => {
    const blocks = parseMarkdown('> zitiert\n> **fett**\n\n```ts\nconst a = "<b>";\n# kein Titel\n```\n\nDanach');
    expect(kinds(blocks)).toEqual(['quote', 'code', 'paragraph']);
    expect(blocks[1]).toEqual({ t: 'code', v: 'const a = "<b>";\n# kein Titel', lang: 'ts' });
  });

  it('schließt einen offenen Codeblock am Ende', () => {
    expect(parseMarkdown('```\nnie geschlossen')).toEqual([{ t: 'code', v: 'nie geschlossen', lang: '' }]);
  });

  it('kommt mit leerem Text, Windows-Zeilenenden und nur Leerraum zurecht', () => {
    expect(parseMarkdown('')).toEqual([]);
    expect(parseMarkdown('  \n\n  ')).toEqual([]);
    expect(kinds(parseMarkdown('# A\r\n\r\ntext\r\n- x'))).toEqual(['heading', 'paragraph', 'list']);
  });

  it('beendet einen Absatz vor einer Liste', () => {
    expect(kinds(parseMarkdown('Text\n- punkt'))).toEqual(['paragraph', 'list']);
  });
});

describe('Aufgaben', () => {
  const source = '# Ergebnis\n- [ ] Konzept an die Schule schicken\n- [x] Termin vereinbart\n  - [ ] Unteraufgabe mit [[Gymnasium Nord]]\n- normaler Punkt\n\n```\n- [ ] in Code\n```';

  it('findet Aufgaben mit Zeile und Erledigt-Kennzeichen, auch verschachtelt', () => {
    expect(extractTasks(source)).toEqual([
      { line: 1, done: false, text: 'Konzept an die Schule schicken' },
      { line: 2, done: true, text: 'Termin vereinbart' },
      { line: 3, done: false, text: 'Unteraufgabe mit Gymnasium Nord' },
    ]);
  });

  it('ändert das Kennzeichen der richtigen Zeile', () => {
    const changed = setTaskDone(source, 1, true).split('\n');
    expect(changed[1]).toBe('- [x] Konzept an die Schule schicken');
    expect(changed[2]).toBe('- [x] Termin vereinbart');
    expect(setTaskDone(source, 2, false).split('\n')[2]).toBe('- [ ] Termin vereinbart');
    expect(setTaskDone(source, 99, true)).toBe(source);
  });
});

describe('wikilinks', () => {
  it('sammelt Titel ohne Wiederholung und ignoriert Code', () => {
    const source = 'Mit [[Grundschule am See]] und [[Frau Weber|ihr]], nochmal [[Grundschule am See]].\n\n`[[im Code]]`\n\n```\n[[im Block]]\n```\n- Liste mit [[Gymnasium Nord]]';
    expect(wikilinks(source)).toEqual(['Grundschule am See', 'Frau Weber', 'Gymnasium Nord']);
    expect(wikilinks('')).toEqual([]);
  });
});
