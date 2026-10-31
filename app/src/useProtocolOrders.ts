import { useCallback, useEffect, useState } from 'preact/hooks';
import { api } from './api';
import type { RecordView } from './types';

/**
 * Aufträge, die aus einem Protokoll entstanden sind. Ein Auftrag verweist über das Feld `protokoll` auf sein
 * Protokoll. Das Protokoll muss gespeichert sein, sonst gibt es noch keine ID zum Verweisen.
 */
export function useProtocolOrders(protocol: RecordView | null, onChanged: () => void) {
  const [existing, setExisting] = useState<RecordView[]>([]);
  const protocolId = protocol?.id ?? null;

  const reload = useCallback(async () => {
    if (!protocolId) {
      setExisting([]);
      return;
    }
    try {
      const all = await api.list('auftrag');
      setExisting(all.filter((a) => a.fields.protokoll === protocolId));
    } catch {
      setExisting([]);
    }
  }, [protocolId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  /** Vorhandener Auftrag mit genau diesem Titel aus diesem Protokoll. */
  const find = (title: string): RecordView | undefined => existing.find((a) => a.fields.titel === title.trim());

  /** Legt einen Auftrag aus Text an. Schule und Datum des Auftrags stammen vom gespeicherten Protokoll. */
  const create = async (text: string): Promise<string> => {
    if (!protocol) throw new Error('Das Protokoll muss zuerst gespeichert werden.');
    const title = text.trim().replace(/\s+/g, ' ').slice(0, 200);
    const { id } = await api.save('auftrag', null, {
      titel: title,
      protokoll: protocol.id,
      schule: typeof protocol.fields.schule === 'string' ? protocol.fields.schule : null,
      erteilt_am: typeof protocol.fields.datum === 'string' ? protocol.fields.datum : null,
    });
    await reload();
    onChanged();
    return id;
  };

  return { existing, find, create };
}
