// Ende-zu-Ende-Test: Browser-Oberfläche (Vite) -> HTTP-Brücke -> Rust-Kern -> Go-Server.
//
// Er baut `dev_bridge` und den Go-Server, startet beides sowie Vite auf freien Ports und steuert die
// Oberfläche mit Playwright (Chromium). Aufruf: `npm run e2e` im Ordner `app/`.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');

const here = dirname(fileURLToPath(import.meta.url));
const appDir = join(here, '..');
const repo = join(appDir, '..');
const shots = process.env.BDB_SHOTS ?? '';
if (shots) mkdirSync(shots, { recursive: true });

const PASSWORD = 'korrekt pferd batterie heftklammer';
const TOKEN = 'e2e-token-e2e-token-e2e-token-e2e-token';
const children = [];
const tempDirs = [];

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { stdio: 'inherit', ...opts });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} ist fehlgeschlagen`);
}

function freePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
    srv.on('error', reject);
  });
}

function start(cmd, args, opts) {
  const child = spawn(cmd, args, { stdio: ['ignore', 'pipe', 'pipe'], ...opts });
  let output = '';
  child.stdout.on('data', (d) => (output += d));
  child.stderr.on('data', (d) => (output += d));
  child.output = () => output;
  children.push(child);
  return child;
}

async function waitFor(url, what, init) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url, init);
      if (r.status < 500) return;
    } catch {
      /* noch nicht bereit */
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`${what} wurde nicht bereit`);
}

function tempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  tempDirs.push(dir);
  return dir;
}

async function startBridge(binary) {
  const port = await freePort();
  start(binary, [tempDir('bdb-e2e-data-'), String(port)], { env: { ...process.env, BDB_FAST_KDF: '1' } });
  await waitFor(`http://127.0.0.1:${port}/api/status`, 'Brücke', { method: 'POST' });
  return port;
}

async function bridgeCall(port, command, args) {
  const r = await fetch(`http://127.0.0.1:${port}/api/${command}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: args === undefined ? '' : JSON.stringify(args),
  });
  const body = await r.json();
  assert.ok(body.ok, `${command}: ${JSON.stringify(body.error)}`);
  return body.data;
}

const results = [];
async function step(name, fn) {
  const started = Date.now();
  try {
    await fn();
    results.push({ name, ok: true });
    console.log(`  ok   ${name} (${Date.now() - started} ms)`);
  } catch (e) {
    results.push({ name, ok: false, error: e });
    console.log(`  FAIL ${name}\n       ${String(e.message ?? e).split('\n').join('\n       ')}`);
    throw e;
  }
}

async function shot(page, name) {
  if (shots) await page.screenshot({ path: join(shots, `${name}.png`), fullPage: true });
}

async function main() {
  console.log('Baue Kern und Server …');
  run('cargo', ['build', '-p', 'bdb-core', '--bin', 'dev_bridge'], { cwd: repo });
  const bridgeBinary = join(repo, 'target', 'debug', 'dev_bridge');
  const serverBinary = join(tempDir('bdb-e2e-bin-'), 'bdbb-server');
  run('go', ['build', '-o', serverBinary, '.'], { cwd: join(repo, 'server') });

  // Go-Server
  const serverPort = await freePort();
  start(serverBinary, [], {
    env: { ...process.env, BDBB_TOKEN: TOKEN, BDBB_ADDR: `127.0.0.1:${serverPort}`, BDBB_DB: join(tempDir('bdb-e2e-server-'), 'server.db') },
  });
  const serverUrl = `http://127.0.0.1:${serverPort}`;
  await waitFor(`${serverUrl}/healthz`, 'Go-Server');

  // Zwei Kerne: A wird über die Oberfläche bedient, B simuliert einen zweiten Rechner über die Brücke.
  const bridgeA = await startBridge(bridgeBinary);
  const bridgeB = await startBridge(bridgeBinary);

  const vitePort = await freePort();
  const vite = start('npx', ['vite', '--port', String(vitePort), '--host', '127.0.0.1'], {
    cwd: appDir,
    env: { ...process.env, BDB_DEV_PORT: String(vitePort), BDB_BRIDGE: `http://127.0.0.1:${bridgeA}` },
  });
  await waitFor(`http://127.0.0.1:${vitePort}/`, 'Vite');
  const url = `http://localhost:${vitePort}/`;

  const browser = await chromium.launch();
  const context = await browser.newContext({ locale: 'de-DE', viewport: { width: 1200, height: 800 } });
  const page = await context.newPage();
  const consoleErrors = [];
  page.on('console', (m) => m.type() === 'error' && consoleErrors.push(m.text()));
  page.on('pageerror', (e) => consoleErrors.push(String(e)));
  page.setDefaultTimeout(10_000);

  let recoveryKey = '';

  try {
    await step('Ersteinrichtung: Eingaben werden geprüft', async () => {
      await page.goto(url);
      await page.getByRole('heading', { name: 'BdB-Butler' }).waitFor();
      await shot(page, '01-willkommen');

      await page.getByLabel('Master-Passwort', { exact: true }).fill('kurz');
      await page.getByRole('button', { name: 'Tresor anlegen' }).click();
      await page.getByRole('alert').filter({ hasText: 'mindestens 10 Zeichen' }).waitFor();

      await page.getByLabel('Master-Passwort', { exact: true }).fill(PASSWORD);
      await page.getByLabel('Master-Passwort wiederholen').fill('etwas ganz anderes hier');
      await page.getByRole('button', { name: 'Tresor anlegen' }).click();
      await page.getByRole('alert').filter({ hasText: 'stimmen nicht überein' }).waitFor();
    });

    await step('Ersteinrichtung mit Server zeigt den Recovery-Key', async () => {
      await page.getByLabel('Server-Adresse').fill(serverUrl);
      await page.getByLabel('Token').fill(TOKEN);
      await page.getByLabel('Master-Passwort wiederholen').fill(PASSWORD);
      await page.getByRole('button', { name: 'Tresor anlegen' }).click();
      await page.getByRole('heading', { name: 'Recovery-Key' }).waitFor();
      recoveryKey = (await page.getByTestId('recovery-key').innerText()).replace(/\s+/g, '').toUpperCase();
      assert.equal(recoveryKey.length, 52, `Recovery-Key hat ${recoveryKey.length} Zeichen`);
      assert.match(recoveryKey, /^[A-Z2-7]+$/);
      await shot(page, '02-recovery-key');

      const next = page.getByRole('button', { name: 'Weiter' });
      assert.equal(await next.isDisabled(), true, '„Weiter“ muss gesperrt sein, bis bestätigt wurde');
      await page.getByLabel('Ich habe den Recovery-Key sicher aufbewahrt').check();
      await next.click();
      await page.getByRole('navigation', { name: 'Bereiche' }).waitFor();
    });

    await step('Schule anlegen, suchen, ändern', async () => {
      await page.getByRole('button', { name: 'Schule anlegen' }).click();
      await page.getByLabel('Name *').fill('Grundschule am See');
      await page.getByLabel('Schulart').selectOption('Grundschule');
      await page.getByLabel('Ort').fill('Ulm');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('region', { name: 'Schulen' }).getByRole('button', { name: 'Grundschule am See' }).waitFor();

      await page.getByRole('button', { name: 'Schule anlegen' }).click();
      await page.getByLabel('Name *').fill('Gymnasium Nord');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('region', { name: 'Schulen' }).getByRole('button', { name: 'Gymnasium Nord' }).waitFor();

      await page.getByLabel('Suchen').fill('ulm');
      const list = page.getByRole('region', { name: 'Schulen' });
      await list.getByRole('button', { name: 'Grundschule am See' }).waitFor();
      assert.equal(await list.getByRole('button', { name: 'Gymnasium Nord' }).count(), 0, 'Suche muss filtern');
      await page.getByLabel('Suchen').fill('');

      await list.getByRole('button', { name: 'Grundschule am See' }).click();
      await page.getByLabel('Ort').fill('Neu-Ulm');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Grundschule am See' }).waitFor();
      assert.equal(await page.getByLabel('Ort').inputValue(), 'Neu-Ulm');
      await shot(page, '03-schulen');
    });

    await step('Pflichtfelder und Fehler werden angezeigt', async () => {
      await page.getByRole('button', { name: 'Schule anlegen' }).click();
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('alert').filter({ hasText: 'Pflichtfeld' }).waitFor();
      await page.getByRole('button', { name: 'Abbrechen' }).click();
    });

    await step('Ansprechpartner mit Verweis auf die Schule', async () => {
      await page.getByRole('navigation').getByRole('button', { name: 'Ansprechpartner' }).click();
      await page.getByRole('button', { name: 'Ansprechpartner anlegen' }).click();
      await page.getByText('Enthält personenbezogene Daten').waitFor();
      await page.getByLabel('Name *').fill('Frau Weber');
      await page.getByLabel('Rolle').selectOption('Schulleitung');
      await page.getByLabel('Schule').selectOption({ label: 'Grundschule am See' });
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Frau Weber' }).waitFor();
      assert.equal(await page.getByLabel('Rolle').inputValue(), 'Schulleitung');
      await shot(page, '04-ansprechpartner');
    });

    await step('Löschen braucht eine Bestätigung', async () => {
      await page.getByRole('button', { name: 'Löschen' }).click();
      await page.getByRole('button', { name: 'Doch nicht' }).click();
      await page.getByRole('button', { name: 'Löschen' }).click();
      await page.getByRole('button', { name: 'Wirklich löschen' }).click();
      await page.getByText('Noch nichts angelegt.').waitFor();
    });

    await step('Sperren und Entsperren, falsches Passwort wird abgelehnt', async () => {
      await page.getByRole('button', { name: 'Sperren' }).click();
      await page.getByText('Die App ist gesperrt.').waitFor();
      await page.getByLabel('Master-Passwort').fill('falsches passwort hier');
      await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
      await page.getByRole('alert').filter({ hasText: 'falsch' }).waitFor();
      await page.getByLabel('Master-Passwort').fill(PASSWORD);
      await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
      await page.getByRole('navigation', { name: 'Bereiche' }).waitFor();
      await page.getByRole('button', { name: 'Gymnasium Nord' }).waitFor().catch(() => {});
    });

    await step('Entsperren mit dem Recovery-Key', async () => {
      await page.getByRole('button', { name: 'Sperren' }).click();
      await page.getByRole('button', { name: 'Mit Recovery-Key entsperren' }).click();
      await page.getByLabel('Recovery-Key').fill('kein gültiger schlüssel');
      await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
      await page.getByRole('alert').filter({ hasText: 'ungültiges Format' }).waitFor();
      await page.getByLabel('Recovery-Key').fill(recoveryKey.toLowerCase().replace(/(.{4})/g, '$1 ').trim());
      await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
      await page.getByRole('navigation', { name: 'Bereiche' }).waitFor();
    });

    await step('Abgleich: Daten eines zweiten Rechners erscheinen', async () => {
      // Zweiter Rechner (Kern B) tritt über die Brücke bei und legt eine Schule an.
      await bridgeCall(bridgeB, 'config_set', { server_url: serverUrl, token: TOKEN, attachments_dir: '' });
      await bridgeCall(bridgeB, 'join_vault', { password: PASSWORD });
      await bridgeCall(bridgeB, 'sync');
      await bridgeCall(bridgeB, 'save', { kind: 'schule', fields: { name: 'Schule vom zweiten Rechner', ort: 'Bonn' } });
      await bridgeCall(bridgeB, 'sync');

      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
      await page.getByTestId('sync-status').filter({ hasText: 'Abgeglichen' }).waitFor();
      await page.getByRole('navigation').getByRole('button', { name: 'Schulen' }).click();
      await page.getByRole('button', { name: 'Schule vom zweiten Rechner' }).waitFor();
      await shot(page, '05-abgleich');
    });

    await step('Abgleich: Änderung von hier erreicht den zweiten Rechner', async () => {
      await page.getByRole('button', { name: 'Schule anlegen' }).click();
      await page.getByLabel('Name *').fill('Schule von der Oberfläche');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Schule von der Oberfläche' }).waitFor();
      // Der automatische Abgleich läuft kurz nach der Änderung.
      await page.getByTestId('sync-status').filter({ hasText: 'Abgeglichen' }).waitFor();
      const deadline = Date.now() + 15_000;
      let names = [];
      while (Date.now() < deadline) {
        await bridgeCall(bridgeB, 'sync');
        names = (await bridgeCall(bridgeB, 'list', { kind: 'schule' })).map((r) => r.fields.name);
        if (names.includes('Schule von der Oberfläche')) break;
        await new Promise((r) => setTimeout(r, 500));
      }
      assert.ok(names.includes('Schule von der Oberfläche'), `B sieht: ${names.join(', ')}`);
    });

    await step('Konflikt: gleiches Feld auf zwei Rechnern geändert', async () => {
      const list = await bridgeCall(bridgeB, 'list', { kind: 'schule' });
      const id = list.find((r) => r.fields.name === 'Schule vom zweiten Rechner').id;

      // B ändert den Ort und sendet. Danach ändert die Oberfläche (A) denselben Ort, ohne vorher abzugleichen.
      await bridgeCall(bridgeB, 'save', { kind: 'schule', id, fields: { ort: 'Köln' } });
      await bridgeCall(bridgeB, 'sync');
      await page.getByRole('region', { name: 'Schulen' }).getByRole('button', { name: 'Schule vom zweiten Rechner' }).click();
      await page.getByLabel('Ort').fill('Aachen');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Schule vom zweiten Rechner' }).waitFor();
      // A holt jetzt den Stand von B: gleiches Feld, beide neu -> Konflikt.
      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
      await page.getByRole('navigation').getByRole('button', { name: /^Konflikte \(1\)$/ }).waitFor();
      await page.getByRole('navigation').getByRole('button', { name: /^Konflikte/ }).click();
      await page.getByText('Feld „ort“').waitFor();
      await shot(page, '06-konflikt');
      await page.getByRole('button', { name: 'Aktuellen Wert behalten' }).click();
      await page.getByText('Keine offenen Konflikte.').waitFor();
    });

    await step('Einstellungen: Passwort ändern', async () => {
      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      await page.getByLabel('Aktuelles Passwort').fill('falsches passwort hier');
      await page.getByLabel('Neues Passwort', { exact: true }).fill('ein neues langes passwort');
      await page.getByLabel('Neues Passwort wiederholen').fill('ein neues langes passwort');
      await page.getByRole('button', { name: 'Passwort ändern' }).click();
      await page.getByRole('alert').filter({ hasText: 'falsch' }).waitFor();

      await page.getByLabel('Aktuelles Passwort').fill(PASSWORD);
      await page.getByRole('button', { name: 'Passwort ändern' }).click();
      await page.getByText('Das Master-Passwort wurde geändert').waitFor();
      await shot(page, '07-einstellungen');

      await page.getByRole('button', { name: 'Sperren' }).click();
      await page.getByLabel('Master-Passwort').fill(PASSWORD);
      await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
      await page.getByRole('alert').filter({ hasText: 'falsch' }).waitFor();
      await page.getByLabel('Master-Passwort').fill('ein neues langes passwort');
      await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
      await page.getByRole('navigation', { name: 'Bereiche' }).waitFor();
    });

    await step('Keine unerwarteten Fehler in der Browser-Konsole', async () => {
      // Die Brücke antwortet auf Fehler mit HTTP 400, die der Test absichtlich auslöst (falsches Passwort,
      // fehlendes Pflichtfeld, ungültiger Recovery-Key). Der Browser protokolliert sie als Ladefehler.
      const expected = /^Failed to load resource: the server responded with a status of 400/;
      const unexpected = consoleErrors.filter((m) => !expected.test(m));
      assert.deepEqual(unexpected, [], unexpected.join('\n'));
      const provoked = consoleErrors.length - unexpected.length;
      assert.equal(provoked, 5, `${provoked} statt 5 erwarteter Fehlerantworten`);
    });
  } finally {
    await browser.close();
  }
  void vite;
}

let failed = false;
try {
  await main();
} catch (e) {
  failed = true;
  if (!results.some((r) => !r.ok)) console.error(e);
} finally {
  for (const child of children) child.kill();
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
}
const passed = results.filter((r) => r.ok).length;
console.log(`\n${passed} von ${results.length} Schritten erfolgreich`);
process.exit(failed ? 1 : 0);
