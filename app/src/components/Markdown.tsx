import type { ComponentChildren, VNode } from 'preact';
import { parseMarkdown, type Block, type Inline, type ListItem } from '../markdown';

export interface WikiTarget {
  kind: string;
  id: string;
}

export interface MarkdownTask {
  line: number;
  done: boolean;
  text: string;
}

interface Props {
  source: string;
  /// Löst den Titel eines Wikilinks auf einen Eintrag auf (`null`, wenn es keinen gibt oder er mehrdeutig ist).
  resolve: (title: string) => WikiTarget | null;
  onOpen: (target: WikiTarget) => void;
  /// Schaltfläche neben einer Aufgabe, zum Beispiel „Auftrag anlegen“.
  taskAction?: (task: MarkdownTask) => ComponentChildren;
  /// Erledigt-Kennzeichen einer Aufgabe ändern.
  onToggleTask?: (line: number, done: boolean) => void;
}

/** Zeigt Markdown an. Es entstehen nur Preact-Knoten, nie HTML aus der Eingabe. */
export function Markdown({ source, resolve, onOpen, taskAction, onToggleTask }: Props) {
  const blocks = parseMarkdown(source);
  const ctx: Ctx = { resolve, onOpen, taskAction, onToggleTask };
  return <div class="markdown">{blocks.map((b, i) => renderBlock(b, i, ctx))}</div>;
}

type Ctx = Pick<Props, 'resolve' | 'onOpen' | 'taskAction' | 'onToggleTask'>;

function renderInline(nodes: Inline[], ctx: Ctx): ComponentChildren[] {
  return nodes.map((n, i): ComponentChildren => {
    switch (n.t) {
      case 'text':
        return n.v;
      case 'br':
        return <br key={i} />;
      case 'code':
        return <code key={i}>{n.v}</code>;
      case 'strong':
        return <strong key={i}>{renderInline(n.c, ctx)}</strong>;
      case 'em':
        return <em key={i}>{renderInline(n.c, ctx)}</em>;
      case 'link':
        return (
          <a key={i} href={n.href} target="_blank" rel="noopener noreferrer">
            {renderInline(n.c, ctx)}
          </a>
        );
      case 'wikilink': {
        const target = ctx.resolve(n.title);
        return target ? (
          <button key={i} type="button" class="wikilink" onClick={() => ctx.onOpen(target)}>
            {n.label}
          </button>
        ) : (
          <span key={i} class="wikilink missing" title="Kein eindeutiger Eintrag mit diesem Titel">
            {n.label}
          </span>
        );
      }
    }
  });
}

function renderItem(item: ListItem, ctx: Ctx): VNode {
  const text = inlineText(item.c);
  return (
    <li key={item.line} class={item.task !== null ? 'task' : undefined}>
      {item.task !== null && (
        <input
          type="checkbox"
          checked={item.task}
          disabled={!ctx.onToggleTask}
          aria-label={`Aufgabe: ${text}`}
          onChange={(e) => ctx.onToggleTask?.(item.line, e.currentTarget.checked)}
        />
      )}
      {renderInline(item.c, ctx)}
      {item.task !== null && ctx.taskAction?.({ line: item.line, done: item.task, text })}
      {item.children.map((child, i) => renderBlock(child, i, ctx))}
    </li>
  );
}

function inlineText(nodes: Inline[]): string {
  return nodes
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
    .join('')
    .trim();
}

function renderBlock(block: Block, key: number, ctx: Ctx): VNode {
  switch (block.t) {
    case 'heading': {
      const Tag = `h${Math.min(6, block.level + 1)}` as 'h2';
      return <Tag key={key}>{renderInline(block.c, ctx)}</Tag>;
    }
    case 'paragraph':
      return <p key={key}>{renderInline(block.c, ctx)}</p>;
    case 'list': {
      const Tag = block.ordered ? 'ol' : 'ul';
      return <Tag key={key}>{block.items.map((item) => renderItem(item, ctx))}</Tag>;
    }
    case 'quote':
      return <blockquote key={key}>{block.c.map((b, i) => renderBlock(b, i, ctx))}</blockquote>;
    case 'code':
      return (
        <pre key={key}>
          <code>{block.v}</code>
        </pre>
      );
    case 'hr':
      return <hr key={key} />;
  }
}
