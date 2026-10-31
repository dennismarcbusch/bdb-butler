import { afterEach, describe, expect, it, vi } from 'vitest';
import { api, BackendError, call } from './api';

function mockFetch(response: unknown, ok = true) {
  const fetchMock = vi.fn().mockResolvedValue({ ok, json: async () => response });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

afterEach(() => vi.unstubAllGlobals());

describe('Aufruf über die Brücke', () => {
  it('sendet den Befehl als POST und liefert die Daten', async () => {
    const fetchMock = mockFetch({ ok: true, data: { unlocked: true } });
    const result = await call('status');
    expect(result).toEqual({ unlocked: true });
    expect(fetchMock).toHaveBeenCalledWith('/api/status', expect.objectContaining({ method: 'POST', body: '' }));
  });

  it('übergibt Argumente als JSON', async () => {
    const fetchMock = mockFetch({ ok: true, data: { id: 'x' } });
    await api.save('schule', null, { name: 'A' });
    expect(fetchMock.mock.calls[0]?.[1].body).toBe(JSON.stringify({ kind: 'schule', fields: { name: 'A' } }));
    await api.save('schule', 'abc', { name: 'B' });
    expect(fetchMock.mock.calls[1]?.[1].body).toBe(JSON.stringify({ kind: 'schule', id: 'abc', fields: { name: 'B' } }));
  });

  it('wandelt Fehler des Kerns in BackendError mit Code', async () => {
    mockFetch({ ok: false, error: { code: 'wrong_secret', message: 'Passwort falsch' } });
    await expect(call('unlock', {})).rejects.toMatchObject({ name: 'BackendError', code: 'wrong_secret', message: 'Passwort falsch' });
  });

  it('meldet einen nicht erreichbaren Kern', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    const error = (await call('status').catch((e: unknown) => e)) as BackendError;
    expect(error).toBeInstanceOf(BackendError);
    expect(error.code).toBe('bridge');
  });

  it('meldet ungültige Antworten', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => { throw new Error('kein JSON'); } }));
    await expect(call('status')).rejects.toMatchObject({ code: 'bridge' });
  });
});
