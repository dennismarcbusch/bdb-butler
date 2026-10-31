import type { ComponentChildren } from 'preact';
import { useEffect, useId, useMemo, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import { emptyValues, toPayload, titleOf, valuesFromRecord, type FormValue, type FormValues } from '../form';
import { titleIndex } from '../graph';
import type { FieldDef, KindDef, RecordView } from '../types';
import { useProtocolOrders } from '../useProtocolOrders';
import { MarkdownField } from './MarkdownField';

interface Props {
  kind: KindDef;
  kinds: KindDef[];
  record: RecordView | null;
  onSaved: (id: string) => void;
  onDeleted: () => void;
  onCancel: () => void;
  onNavigate: (kind: string, id: string) => void;
  /// Aufträge wurden angelegt (Listen und Zähler neu laden).
  onOrdersChanged?: () => void;
}

interface Option {
  id: string;
  title: string;
}

export function RecordForm({ kind, kinds, record, onSaved, onDeleted, onCancel, onNavigate, onOrdersChanged }: Props) {
  const [values, setValues] = useState<FormValues>(() => (record ? valuesFromRecord(kind, record) : emptyValues(kind)));
  const [options, setOptions] = useState<Record<string, Option[]>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  // Für Wikilinks in Markdown-Feldern: alle Einträge mit Titel.
  const [all, setAll] = useState<RecordView[]>([]);
  const hasMarkdown = kind.fields.some((f) => f.type === 'markdown');
  useEffect(() => {
    if (hasMarkdown) api.listAll().then(setAll, () => undefined);
  }, [hasMarkdown]);
  // Aufträge aus Protokollen (nur bei Protokollen).
  const isProtocol = kind.kind === 'protokoll';
  const orders = useProtocolOrders(isProtocol ? record : null, () => onOrdersChanged?.());
  const createOrder = async (text: string) => {
    setError(null);
    try {
      await orders.create(text);
    } catch (e) {
      setError(messageOf(e));
    }
  };
  const resolve = useMemo(() => {
    const index = titleIndex(all, kinds);
    const byId = new Map(all.map((r) => [r.id, r]));
    return (title: string) => {
      const id = index.get(title.trim().toLowerCase());
      const found = id ? byId.get(id) : undefined;
      return found ? { kind: found.kind, id: found.id } : null;
    };
  }, [all, kinds]);

  // Auswahllisten für Verweise laden (einmal je Zieltyp).
  useEffect(() => {
    const targets = new Set(
      kind.fields.flatMap((f) => (f.type === 'ref' || f.type === 'ref_list' ? [f.kind] : [])),
    );
    for (const target of targets) {
      const def = kinds.find((k) => k.kind === target);
      if (!def) continue;
      api.list(target).then(
        (list) =>
          setOptions((prev) => ({
            ...prev,
            [target]: list
              // Ein Eintrag verweist nicht auf sich selbst.
              .filter((r) => r.id !== record?.id)
              .map((r) => ({ id: r.id, title: titleOf(def, r) })),
          })),
        (e) => setError(messageOf(e)),
      );
    }
  }, [kind, kinds, record?.id]);

  const set = (name: string, value: FormValue) => setValues((v) => ({ ...v, [name]: value }));

  async function submit(event: Event) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const { id } = await api.save(kind.kind, record?.id ?? null, toPayload(kind, values));
      onSaved(id);
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!record) return;
    setBusy(true);
    try {
      await api.delete(record.id);
      onDeleted();
    } catch (e) {
      setError(messageOf(e));
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate>
      <h2>{record ? titleOf(kind, record) : `${kind.label} anlegen`}</h2>
      {kind.personal && (
        <p class="muted">Enthält personenbezogene Daten. Für die KI gesperrt, solange du es nicht freigibst.</p>
      )}
      {kind.fields.map((field) => (
        field.type === 'markdown' ? (
          <MarkdownField
            key={field.name}
            label={field.required ? `${field.label} *` : field.label}
            value={typeof values[field.name] === 'string' ? (values[field.name] as string) : ''}
            onChange={(v) => set(field.name, v)}
            resolve={resolve}
            onOpen={(target) => onNavigate(target.kind, target.id)}
            taskAction={
              isProtocol
                ? (task) => {
                    const found = orders.find(task.text);
                    return found ? (
                      <button type="button" class="link small" onClick={() => onNavigate('auftrag', found.id)}>
                        Auftrag öffnen
                      </button>
                    ) : (
                      <button
                        type="button"
                        class="small"
                        disabled={!record}
                        title={record ? undefined : 'Das Protokoll zuerst speichern'}
                        onClick={() => void createOrder(task.text)}
                      >
                        Auftrag anlegen
                      </button>
                    );
                  }
                : undefined
            }
            selectionAction={
              isProtocol
                ? {
                    label: 'Auftrag aus Markierung anlegen',
                    disabledReason: record ? undefined : 'Das Protokoll zuerst speichern',
                    run: createOrder,
                  }
                : undefined
            }
          />
        ) : (
          <Field key={field.name} field={field} value={values[field.name]} options={refOptions(field, options)} onChange={(v) => set(field.name, v)} />
        )
      ))}
      {error && <p class="error" role="alert">{error}</p>}
      <div class="row">
        <button type="submit" class="primary" disabled={busy}>Speichern</button>
        <button type="button" onClick={onCancel} disabled={busy}>Abbrechen</button>
        {record && !confirmDelete && (
          <button type="button" class="danger" onClick={() => setConfirmDelete(true)} disabled={busy}>Löschen</button>
        )}
        {record && confirmDelete && (
          <>
            <button type="button" class="danger" onClick={() => void remove()} disabled={busy}>Wirklich löschen</button>
            <button type="button" onClick={() => setConfirmDelete(false)} disabled={busy}>Doch nicht</button>
          </>
        )}
      </div>
    </form>
  );
}

function refOptions(field: FieldDef, all: Record<string, Option[]>): Option[] {
  return field.type === 'ref' || field.type === 'ref_list' ? (all[field.kind] ?? []) : [];
}

interface FieldProps {
  field: FieldDef;
  value: FormValue | undefined;
  options: Option[];
  onChange: (value: FormValue) => void;
}

function Field({ field, value, options, onChange }: FieldProps) {
  // Label und Steuerelement sind über `for` und `id` verbunden. Ein umschließendes Label würde bei
  // Auswahlfeldern den Text aller Optionen in den Namen des Feldes aufnehmen.
  const id = useId();
  const label = field.required ? `${field.label} *` : field.label;
  const text = typeof value === 'string' ? value : '';
  const wrap = (control: ComponentChildren) => (
    <div class="field">
      <label for={id}>{label}</label>
      {control}
    </div>
  );

  switch (field.type) {
    case 'long_text':
      return wrap(<textarea id={id} rows={8} value={text} onInput={(e) => onChange(e.currentTarget.value)} />);
    case 'bool':
      return (
        <label class="check">
          <input type="checkbox" checked={value === true} onChange={(e) => onChange(e.currentTarget.checked)} />
          {label}
        </label>
      );
    case 'choice':
      return wrap(
        <select id={id} value={text} onChange={(e) => onChange(e.currentTarget.value)}>
          <option value="">–</option>
          {field.options.map((o) => (
            <option key={o} value={o}>{o}</option>
          ))}
        </select>,
      );
    case 'ref':
      return wrap(
        <select id={id} value={text} onChange={(e) => onChange(e.currentTarget.value)}>
          <option value="">–</option>
          {options.map((o) => (
            <option key={o.id} value={o.id}>{o.title}</option>
          ))}
        </select>,
      );
    case 'ref_list': {
      const selected = Array.isArray(value) ? value : [];
      return (
        <fieldset>
          <legend>{label}</legend>
          {options.length === 0 && <p class="muted">Keine Einträge zur Auswahl.</p>}
          <div class="checklist">
            {options.map((o) => (
              <label class="check" key={o.id}>
                <input
                  type="checkbox"
                  checked={selected.includes(o.id)}
                  onChange={(e) =>
                    onChange(e.currentTarget.checked ? [...selected, o.id] : selected.filter((x) => x !== o.id))
                  }
                />
                {o.title}
              </label>
            ))}
          </div>
        </fieldset>
      );
    }
    case 'date':
      return wrap(<input id={id} type="date" value={text} onInput={(e) => onChange(e.currentTarget.value)} />);
    case 'date_time':
      return wrap(<input id={id} type="datetime-local" value={text} onInput={(e) => onChange(e.currentTarget.value)} />);
    case 'number':
      return wrap(<input id={id} type="text" inputMode="decimal" value={text} onInput={(e) => onChange(e.currentTarget.value)} />);
    default:
      return wrap(<input id={id} type="text" value={text} onInput={(e) => onChange(e.currentTarget.value)} />);
  }
}
