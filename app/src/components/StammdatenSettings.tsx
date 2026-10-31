import { useEffect, useState } from 'preact/hooks';
import { api } from '../api';
import { messageOf } from '../errors';
import { toPayload, valuesFromRecord, type FormValue, type FormValues } from '../form';
import type { KindDef } from '../types';
import { Field } from './RecordForm';

const GROUPS: { title: string; fields: string[] }[] = [
  { title: 'Persönliche Daten', fields: ['name', 'vorname', 'plz', 'ort', 'strasse', 'telefon', 'email'] },
  { title: 'Dienststelle', fields: ['dienststelle', 'dienststelle_kurz', 'abteilung', 'besoldungsgruppe', 'org_nr', 'personal_nr'] },
  { title: 'Bankverbindung', fields: ['iban', 'bic', 'kreditinstitut'] },
  { title: 'Mailversand', fields: ['empfaenger_antrag', 'empfaenger_abrechnung', 'absender', 'smtp_host', 'smtp_port', 'smtp_user', 'smtp_sicherheit'] },
];

/** Stammdaten für Dienstreiseantrag und Abrechnung, Unterschrift und Mailversand. */
export function StammdatenSettings({ kinds }: { kinds: KindDef[] }) {
  const def = kinds.find((k) => k.kind === 'stammdaten');
  const [values, setValues] = useState<FormValues | null>(null);
  const [passwordSet, setPasswordSet] = useState(false);
  const [password, setPassword] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!def) return;
    api.stammdatenGet().then(
      (s) => {
        setValues(valuesFromRecord(def, { id: s.id, kind: 'stammdaten', fields: s.fields }));
        setPasswordSet(s.smtp_password_set);
      },
      (e) => setError(messageOf(e)),
    );
  }, [def]);

  if (!def) return null;
  if (!values) return <p>{error ?? 'Lade …'}</p>;

  const set = (name: string, value: FormValue) => setValues((v) => ({ ...(v ?? {}), [name]: value }));
  const svg = typeof values.unterschrift_svg === 'string' ? values.unterschrift_svg : '';

  async function loadSvg(file: File | undefined) {
    if (!file) return;
    const text = await file.text();
    if (!/<svg[\s>]/i.test(text)) {
      setError('Das ist keine SVG-Datei.');
      return;
    }
    setError(null);
    set('unterschrift_svg', text);
  }

  async function save(event: Event) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await api.stammdatenSave(toPayload(def!, values!));
      if (password !== '') {
        await api.smtpPasswordSet(password);
        setPassword('');
        setPasswordSet(true);
      }
      setMessage('Gespeichert.');
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearPassword() {
    try {
      await api.smtpPasswordSet('');
      setPasswordSet(false);
      setPassword('');
    } catch (e) {
      setError(messageOf(e));
    }
  }

  return (
    <form class="stammdaten" onSubmit={save} aria-label="Stammdaten">
      <h3>Stammdaten für Dienstreisen</h3>
      <p class="muted">
        Für Antrag, Abrechnung und Mailversand. Die Daten sind wie alles verschlüsselt. Das Mail-Passwort bleibt nur auf diesem Rechner.
      </p>
      {GROUPS.map((group) => (
        <fieldset key={group.title}>
          <legend>{group.title}</legend>
          {group.fields.map((name) => {
            const field = def.fields.find((f) => f.name === name);
            return field ? <Field key={name} field={field} value={values[name]} options={[]} onChange={(v) => set(name, v)} /> : null;
          })}
          {group.title === 'Mailversand' && (
            <div class="field">
              <label for="smtp-passwort">SMTP-Passwort</label>
              <input
                id="smtp-passwort"
                type="password"
                autocomplete="new-password"
                placeholder={passwordSet ? '•••••••• (gesetzt, zum Ändern neu eingeben)' : ''}
                value={password}
                onInput={(e) => setPassword(e.currentTarget.value)}
              />
              <p class="muted">
                {passwordSet ? 'Ein Passwort ist auf diesem Rechner gespeichert.' : 'Noch kein Passwort gespeichert.'}
                {passwordSet && <button type="button" class="link small" onClick={() => void clearPassword()}>Passwort löschen</button>}
              </p>
            </div>
          )}
        </fieldset>
      ))}
      <fieldset>
        <legend>Unterschrift</legend>
        <div class="field">
          <label for="svg-upload">Unterschrift als SVG-Datei laden</label>
          <input id="svg-upload" type="file" accept=".svg,image/svg+xml" onChange={(e) => void loadSvg(e.currentTarget.files?.[0])} />
        </div>
        <Field field={def.fields.find((f) => f.name === 'unterschrift_ort')!} value={values.unterschrift_ort} options={[]} onChange={(v) => set('unterschrift_ort', v)} />
        {svg.trim() !== '' && (
          <div class="signature-preview">
            <img alt="Vorschau der Unterschrift" src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`} />
            <button type="button" class="link small danger" onClick={() => set('unterschrift_svg', '')}>Unterschrift entfernen</button>
          </div>
        )}
      </fieldset>
      {error && <p class="error" role="alert">{error}</p>}
      {message && <p class="ok" role="status">{message}</p>}
      <div class="row">
        <button type="submit" class="primary" disabled={busy}>Stammdaten speichern</button>
      </div>
    </form>
  );
}
