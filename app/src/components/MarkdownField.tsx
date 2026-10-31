import { useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { Markdown, type MarkdownTask, type WikiTarget } from './Markdown';
import { setTaskDone } from '../markdown';

interface Props {
  label: string;
  value: string;
  onChange: (value: string) => void;
  resolve: (title: string) => WikiTarget | null;
  onOpen: (target: WikiTarget) => void;
  taskAction?: (task: MarkdownTask) => ComponentChildren;
}

/** Markdown-Feld mit Bearbeiten und Vorschau. Aufgaben lassen sich in der Vorschau abhaken. */
export function MarkdownField({ label, value, onChange, resolve, onOpen, taskAction }: Props) {
  const [mode, setMode] = useState<'edit' | 'preview'>('edit');
  return (
    <div class="markdown-field">
      <div class="markdown-head">
        <span class="field-label">{label}</span>
        <div class="tabs small" role="tablist" aria-label={`${label}: Ansicht`}>
          <button type="button" role="tab" aria-selected={mode === 'edit'} class={mode === 'edit' ? 'active' : ''} onClick={() => setMode('edit')}>
            Bearbeiten
          </button>
          <button type="button" role="tab" aria-selected={mode === 'preview'} class={mode === 'preview' ? 'active' : ''} onClick={() => setMode('preview')}>
            Vorschau
          </button>
        </div>
      </div>
      {mode === 'edit' ? (
        <textarea
          class="markdown-source"
          rows={16}
          aria-label={label}
          spellcheck
          value={value}
          onInput={(e) => onChange(e.currentTarget.value)}
        />
      ) : (
        <div class="markdown-preview" aria-label={`${label}: Vorschau`}>
          {value.trim() === '' ? (
            <p class="muted">Noch kein Text.</p>
          ) : (
            <Markdown
              source={value}
              resolve={resolve}
              onOpen={onOpen}
              taskAction={taskAction}
              onToggleTask={(line, done) => onChange(setTaskDone(value, line, done))}
            />
          )}
        </div>
      )}
      <p class="muted">Markdown. Verweise auf Einträge: [[Titel]]. Aufgaben: - [ ] Text</p>
    </div>
  );
}
