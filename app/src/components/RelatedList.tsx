import { useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { related, type Related } from '../graph';
import { titleOf } from '../form';
import type { KindDef } from '../types';

interface Props {
  kinds: KindDef[];
  recordId: string;
  revision: number;
  onOpen: (kind: string, id: string) => void;
}

/** Einträge, die mit dem angezeigten Eintrag verknüpft sind (in beide Richtungen). */
export function RelatedList({ kinds, recordId, revision, onOpen }: Props) {
  const [items, setItems] = useState<Related[]>([]);

  useEffect(() => {
    let cancelled = false;
    api.listAll().then(
      (all) => {
        if (!cancelled) setItems(related(all, kinds, recordId));
      },
      () => {
        if (!cancelled) setItems([]);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [kinds, recordId, revision]);

  if (items.length === 0) return null;
  return (
    <div class="related">
      <h3>Verknüpfte Einträge</h3>
      <ul>
        {items.map((item) => {
          const def = kinds.find((k) => k.kind === item.record.kind);
          return (
            <li key={`${item.direction}-${item.record.id}-${item.fieldLabel}`}>
              <button type="button" class="link" onClick={() => onOpen(item.record.kind, item.record.id)}>
                {def ? titleOf(def, item.record) : item.record.id}
              </button>{' '}
              <span class="muted">
                ({def?.label ?? item.record.kind}, {item.direction === 'out' ? item.fieldLabel : `${item.fieldLabel} dort`})
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
