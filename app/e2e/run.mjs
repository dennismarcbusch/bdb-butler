// Ende-zu-Ende-Test: Browser-Oberfläche (Vite) -> HTTP-Brücke -> Rust-Kern -> Go-Server.
//
// Er baut `dev_bridge` und den Go-Server, startet beides sowie Vite auf freien Ports und steuert die
// Oberfläche mit Playwright (Chromium). Aufruf: `npm run e2e` im Ordner `app/`.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { simpleParser } from 'mailparser';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';

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

/** Ruft einen Befehl der Brücke auf und liefert die ganze Antwort (`ok`, `data` oder `error`). */
async function bridgeRaw(port, command, args) {
  const r = await fetch(`http://127.0.0.1:${port}/api/${command}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: args === undefined ? '' : JSON.stringify(args),
  });
  return r.json();
}

async function bridgeCall(port, command, args) {
  const body = await bridgeRaw(port, command, args);
  assert.ok(body.ok, `${command}: ${JSON.stringify(body.error)}`);
  return body.data;
}


/** Ein kleiner SMTP-Server für den Test: nimmt Mails mit Anmeldung (PLAIN oder LOGIN) an und sammelt sie. */
function startSmtp({ user, pass }) {
  const received = [];
  const server = net.createServer((socket) => {
    let buffer = Buffer.alloc(0);
    let state = 'cmd';
    let from = '';
    let to = [];
    let authed = false;
    let loginStep = 0;
    let data = [];
    const send = (line) => socket.write(`${line}\r\n`);
    const readLine = () => {
      const i = buffer.indexOf('\r\n');
      if (i < 0) return null;
      const line = buffer.subarray(0, i);
      buffer = buffer.subarray(i + 2);
      return line;
    };
    send('220 fake ESMTP');
    socket.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      for (let line = readLine(); line !== null; line = readLine()) {
        if (state === 'data') {
          if (line.toString() === '.') {
            received.push({ from, to, authed, raw: Buffer.concat(data) });
            data = [];
            state = 'cmd';
            send('250 queued');
          } else {
            data.push(Buffer.concat([line.subarray(0, 2).toString() === '..' ? line.subarray(1) : line, Buffer.from('\r\n')]));
          }
          continue;
        }
        const text = line.toString();
        const upper = text.toUpperCase();
        if (state === 'login-user') {
          state = 'login-pass';
          loginStep = Buffer.from(text, 'base64').toString() === user ? 1 : 0;
          send('334 UGFzc3dvcmQ6');
        } else if (state === 'login-pass') {
          state = 'cmd';
          authed = loginStep === 1 && Buffer.from(text, 'base64').toString() === pass;
          send(authed ? '235 ok' : '535 5.7.8 credentials invalid');
        } else if (upper.startsWith('EHLO') || upper.startsWith('HELO')) {
          socket.write('250-fake\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n');
        } else if (upper.startsWith('AUTH PLAIN')) {
          const encoded = text.split(' ')[2] ?? '';
          const [, u, p] = Buffer.from(encoded, 'base64').toString('latin1').split('\0');
          authed = u === user && p === pass;
          send(authed ? '235 ok' : '535 5.7.8 credentials invalid');
        } else if (upper.startsWith('AUTH LOGIN')) {
          state = 'login-user';
          send('334 VXNlcm5hbWU6');
        } else if (upper.startsWith('MAIL FROM')) {
          if (!authed) send('530 authentication required');
          else {
            from = text.slice(10).replace(/[<>]/g, '').trim();
            send('250 ok');
          }
        } else if (upper.startsWith('RCPT TO')) {
          to.push(text.slice(8).replace(/[<>]/g, '').trim());
          send('250 ok');
        } else if (upper === 'DATA') {
          state = 'data';
          send('354 go');
        } else if (upper === 'QUIT') {
          send('221 bye');
          socket.end();
        } else {
          send('250 ok');
        }
      }
    });
    socket.on('error', () => {});
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ port: server.address().port, received, close: () => server.close() })));
}

/** Wartet, bis eine Bedingung zutrifft (Auswahllisten laden nach dem Öffnen eines Formulars asynchron). */
async function until(check, what, timeout = 10_000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try {
      last = await check();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`Zeitüberschreitung: ${what} (zuletzt: ${String(last)})`);
}

const selectedText = (locator) => locator.evaluate((el) => el.selectedOptions[0]?.textContent ?? '');

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

  // Go-Server. Jeder Start bekommt eine neue, leere Datenbank (für „Server neu befüllen“).
  const serverPort = await freePort();
  const serverUrl = `http://127.0.0.1:${serverPort}`;
  let serverProcess = null;
  const startServer = async () => {
    serverProcess = start(serverBinary, [], {
      env: { ...process.env, BDBB_TOKEN: TOKEN, BDBB_ADDR: `127.0.0.1:${serverPort}`, BDBB_DB: join(tempDir('bdb-e2e-server-'), 'server.db') },
    });
    await waitFor(`${serverUrl}/healthz`, 'Go-Server');
  };
  const stopServer = () =>
    new Promise((resolve) => {
      serverProcess.once('exit', resolve);
      serverProcess.kill();
    });
  await startServer();

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
      await page.getByLabel('Schule', { exact: true }).selectOption({ label: 'Grundschule am See' });
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

    await step('Graph: Knoten, Verbindungen, Auswahl, Filter, Suche', async () => {
      // Zwei Ansprechpartner anlegen, beide an einer Schule, untereinander verknüpft.
      const schools = await bridgeCall(bridgeA, 'list', { kind: 'schule' });
      const grund = schools.find((r) => r.fields.name === 'Grundschule am See').id;
      const weber = (await bridgeCall(bridgeA, 'save', { kind: 'ansprechpartner', fields: { name: 'Frau Weber', schule: grund } })).id;
      await bridgeCall(bridgeA, 'save', {
        kind: 'ansprechpartner',
        fields: { name: 'Herr Bauer', schule: grund, verknuepfungen: [weber] },
      });

      await page.getByRole('navigation').getByRole('button', { name: 'Graph' }).click();
      const nodes = page.getByTestId('graph-node');
      await nodes.first().waitFor();
      assert.equal(await nodes.count(), schools.length + 2, 'Knoten: alle Schulen und beide Personen');
      // Weber-Schule, Bauer-Schule, Bauer-Weber
      assert.equal(await page.getByTestId('graph-edge').count(), 3);
      await shot(page, '08-graph');

      // Auswahl zeigt die Verbindungen.
      await page.getByRole('button', { name: 'Ansprechpartner: Frau Weber' }).click();
      const panel = page.getByRole('complementary', { name: 'Auswahl' });
      await panel.getByRole('heading', { name: 'Frau Weber' }).waitFor();
      await panel.getByRole('button', { name: /Grundschule am See/ }).waitFor();
      await panel.getByRole('button', { name: /Herr Bauer/ }).waitFor();
      await shot(page, '09-graph-auswahl');

      // Suche hebt hervor, der Rest wird abgeblendet.
      await page.getByLabel('Im Graph suchen').fill('bauer');
      assert.equal(await page.locator('[data-testid="graph-node"].match').count(), 1);
      assert.ok((await page.locator('[data-testid="graph-node"].dimmed').count()) >= schools.length);
      await page.getByLabel('Im Graph suchen').fill('');

      // Typ ausblenden: Personen und ihre Kanten verschwinden.
      await page.getByRole('group', { name: 'Typen anzeigen' }).getByLabel('Ansprechpartner').uncheck();
      await page.waitForFunction((n) => document.querySelectorAll('[data-testid="graph-node"]').length === n, schools.length);
      assert.equal(await page.getByTestId('graph-edge').count(), 0);
      await page.getByRole('group', { name: 'Typen anzeigen' }).getByLabel('Ansprechpartner').check();
      await nodes.nth(schools.length + 1).waitFor();

      // „Bearbeiten“ öffnet den Eintrag in seiner Typansicht.
      await page.getByRole('button', { name: 'Ansprechpartner: Herr Bauer' }).click();
      await panel.getByRole('button', { name: 'Bearbeiten' }).click();
      await page.getByRole('heading', { name: 'Herr Bauer' }).waitFor();
      assert.equal(await page.getByLabel('Name *').inputValue(), 'Herr Bauer');
      // Verknüpfte Einträge sind sichtbar und führen weiter.
      const related = page.locator('.related');
      await related.getByRole('button', { name: 'Frau Weber' }).waitFor();
      await related.getByRole('button', { name: 'Grundschule am See' }).click();
      await page.getByRole('heading', { name: 'Grundschule am See' }).waitFor();
      await page.locator('.related').getByRole('button', { name: 'Herr Bauer' }).waitFor();
      await shot(page, '10-verknuepft');
    });

    await step('KI-Zugriff: echter MCP-Client, Sperre für Personen, Schreibstufe, Protokoll, Token', async () => {
      const mcpPort = await freePort();
      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      const mcp = page.getByRole('form', { name: 'KI-Zugriff' });
      await mcp.getByTestId('mcp-status').filter({ hasText: 'Der KI-Zugriff ist aus.' }).waitFor();
      await mcp.getByLabel('KI-Zugriff einschalten').check();
      await mcp.getByLabel('Schreibzugriff').check();
      await mcp.getByLabel('Port').fill(String(mcpPort));
      await mcp.getByRole('button', { name: 'Speichern' }).click();
      await mcp.getByTestId('mcp-status').filter({ hasText: 'Läuft auf' }).waitFor();
      await mcp.getByRole('button', { name: 'Anzeigen' }).click();
      const token = (await mcp.getByTestId('mcp-token').innerText()).trim();
      assert.match(token, /^[0-9a-f]{64}$/);
      const mcpUrl = new URL(`http://127.0.0.1:${mcpPort}/mcp`);
      await shot(page, '11-ki-zugriff');

      const connect = async (withToken) => {
        const client = new Client({ name: 'e2e', version: '1.0.0' });
        const transport = new StreamableHTTPClientTransport(mcpUrl, {
          requestInit: { headers: { Authorization: `Bearer ${withToken}` } },
        });
        await client.connect(transport);
        return client;
      };
      const text = (result) => result.content.map((c) => c.text).join('');

      // Ohne gültiges Token kommt niemand hinein.
      await assert.rejects(connect('falsch'), 'ein falsches Token muss abgelehnt werden');

      const client = await connect(token);
      try {
        assert.equal(client.getServerVersion()?.name, 'bdb-butler');
        const tools = (await client.listTools()).tools.map((t) => t.name).sort();
        assert.deepEqual(tools, ['create_record', 'get_record', 'list_kinds', 'list_records', 'search', 'update_record']);

        // Typen und Sperren.
        const kinds = JSON.parse(text(await client.callTool({ name: 'list_kinds', arguments: {} })));
        assert.equal(kinds.write, true);
        assert.equal(kinds.kinds.find((k) => k.kind === 'schule').accessible, true);
        assert.equal(kinds.kinds.find((k) => k.kind === 'ansprechpartner').accessible, false);

        // Lesen und Schreiben bei zugänglichen Typen.
        const created = await client.callTool({
          name: 'create_record',
          arguments: { kind: 'schule', fields: { name: 'Von der KI angelegte Schule', ort: 'Passau' } },
        });
        assert.ok(!created.isError, text(created));
        const createdId = JSON.parse(text(created)).id;
        const found = JSON.parse(text(await client.callTool({ name: 'search', arguments: { query: 'passau' } })));
        assert.equal(found.total, 1);
        await client.callTool({ name: 'update_record', arguments: { id: createdId, fields: { ort: 'Regensburg' } } });
        const got = JSON.parse(text(await client.callTool({ name: 'get_record', arguments: { id: createdId } })));
        assert.equal(got.fields.ort, 'Regensburg');
        const invalid = await client.callTool({ name: 'create_record', arguments: { kind: 'schule', fields: { schulart: 'Zirkus', name: 'X' } } });
        assert.equal(invalid.isError, true);

        // Personen sind gesperrt und tauchen auch in der Suche nicht auf.
        const blocked = await client.callTool({ name: 'list_records', arguments: { kind: 'ansprechpartner' } });
        assert.equal(blocked.isError, true);
        assert.match(text(blocked), /gesperrt/);
        assert.ok(!text(blocked).includes('Weber'), 'Inhalte gesperrter Typen dürfen nicht durchsickern');
        const search = JSON.parse(text(await client.callTool({ name: 'search', arguments: { query: 'weber' } })));
        assert.equal(search.total, 0);

        // Der Nutzer gibt Ansprechpartner frei, danach sind sie lesbar.
        await mcp.getByLabel('Ansprechpartner für die KI freigeben').check();
        await mcp.getByRole('button', { name: 'Speichern' }).click();
        await page.getByText('Gespeichert.').first().waitFor();
        const allowed = JSON.parse(text(await client.callTool({ name: 'list_records', arguments: { kind: 'ansprechpartner' } })));
        assert.deepEqual(allowed.records.map((r) => r.fields.name).sort(), ['Frau Weber', 'Herr Bauer']);

        // Schreibzugriff ausschalten: Werkzeuge verschwinden, Schreiben wird abgelehnt.
        await mcp.getByLabel('Schreibzugriff').uncheck();
        await mcp.getByRole('button', { name: 'Speichern' }).click();
        const readOnlyTools = (await client.listTools()).tools.map((t) => t.name);
        assert.ok(!readOnlyTools.includes('create_record'));
        const denied = await client.callTool({ name: 'create_record', arguments: { kind: 'schule', fields: { name: 'Darf nicht sein' } } });
        assert.equal(denied.isError, true);
        assert.match(text(denied), /Schreibzugriff/);
      } finally {
        await client.close();
      }

      // Das Protokoll zeigt die Zugriffe, aber keine Inhalte.
      await mcp.getByRole('button', { name: 'Aktualisieren' }).click();
      const audit = mcp.getByTestId('mcp-audit');
      await audit.getByRole('cell', { name: 'create_record' }).first().waitFor();
      const auditText = await audit.innerText();
      assert.ok(auditText.includes('abgelehnt'), 'abgelehnte Zugriffe stehen im Protokoll');
      for (const secret of ['Passau', 'Regensburg', 'Von der KI angelegte Schule', 'Frau Weber', 'Darf nicht sein']) {
        assert.ok(!auditText.includes(secret), `Inhalt „${secret}“ im Protokoll`);
      }
      await shot(page, '12-ki-protokoll');

      // Die von der KI angelegte Schule ist in der Oberfläche sichtbar.
      await page.getByRole('navigation').getByRole('button', { name: 'Schulen' }).click();
      await page.getByRole('button', { name: 'Von der KI angelegte Schule' }).waitFor();

      // Neues Token: das alte hat keinen Zugriff mehr.
      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      await mcp.getByRole('button', { name: 'Neues Token erzeugen' }).click();
      await mcp.getByRole('button', { name: 'Wirklich neu erzeugen' }).click();
      await page.getByText('Neues Token erzeugt').waitFor();
      const newToken = (await mcp.getByTestId('mcp-token').innerText()).trim();
      assert.notEqual(newToken, token);
      await assert.rejects(connect(token), 'das alte Token muss ungültig sein');
      const again = await connect(newToken);
      await again.close();
    });

    await step('Beratungen und Protokolle: Markdown, Wikilinks, Aufgaben, Sicherheit', async () => {
      const nav = (name) => page.getByRole('navigation').getByRole('button', { name, exact: true }).click();

      // Beratene Person (personenbezogen) und zwei Beratungen.
      await nav('Beratene Personen');
      await page.getByRole('button', { name: 'Beratene Person anlegen' }).click();
      await page.getByLabel('Name *').fill('Herr Müller');
      await page.getByLabel('Schule', { exact: true }).selectOption({ label: 'Grundschule am See' });
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Herr Müller' }).waitFor();

      await nav('Beratungen');
      await page.getByRole('button', { name: 'Beratung anlegen' }).click();
      await page.getByLabel('Thema *').fill('Tablet-Einsatz im Unterricht');
      await page.getByLabel('Datum *').fill('2026-10-30');
      await page.getByLabel('Art').selectOption('Vor Ort');
      await page.getByLabel('Dauer in Minuten').fill('90');
      await page.getByLabel('Schule', { exact: true }).selectOption({ label: 'Grundschule am See' });
      await page.getByRole('group', { name: 'Beratene Personen' }).getByLabel('Herr Müller').check();
      await page.getByLabel('Inhalt und Ergebnis', { exact: true }).fill('Medienkonzept besprochen.');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Tablet-Einsatz im Unterricht' }).waitFor();

      await page.getByRole('button', { name: 'Beratung anlegen' }).click();
      await page.getByLabel('Thema *').fill('Neuere Beratung');
      await page.getByLabel('Datum *').fill('2026-11-05');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Neuere Beratung' }).waitFor();
      // Jeder Eintrag hat eine Zusatzzeile mit dem Datum, verglichen wird der Titel in der ersten Zeile.
      const titles = (await page.getByRole('region', { name: 'Beratungen' }).locator('li button').allInnerTexts()).map((t) => t.split('\n')[0]);
      assert.deepEqual(titles, ['Neuere Beratung', 'Tablet-Einsatz im Unterricht'], 'neueste Beratung zuerst');
      // Ungültiges Datum wird abgelehnt.
      await page.getByRole('button', { name: 'Beratung anlegen' }).click();
      await page.getByLabel('Thema *').fill('Mit falschem Datum');
      await page.getByLabel('Datum *').fill('');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('alert').filter({ hasText: 'Pflichtfeld' }).waitFor();
      await page.getByRole('button', { name: 'Abbrechen' }).click();

      // Protokoll mit Markdown, Wikilinks, Aufgaben und eingeschleustem Skript.
      await nav('Protokolle');
      await page.getByRole('button', { name: 'Protokoll anlegen' }).click();
      await page.getByText('Enthält personenbezogene Daten').waitFor();
      await page.getByLabel('Titel *').fill('Beratung Tablets');
      await page.getByLabel('Art *').selectOption('Beratung');
      await page.getByLabel('Datum *').fill('2026-10-30');
      await page.getByLabel('Beratung', { exact: true }).selectOption({ label: 'Tablet-Einsatz im Unterricht' });
      const source = [
        '# Ergebnis',
        'Besprochen mit [[Grundschule am See]] und [[Gibt es nicht]].',
        '',
        '**Wichtig:** <script>window.__xss = 1</script> [böse](javascript:alert(1))',
        '',
        '- [ ] Konzept schicken',
        '- [x] Termin vereinbart',
      ].join('\n');
      await page.getByRole('textbox', { name: 'Protokoll (Markdown)' }).fill(source);
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Beratung Tablets' }).waitFor();

      await page.getByRole('tab', { name: 'Vorschau' }).click();
      const preview = page.getByLabel('Protokoll (Markdown): Vorschau');
      await preview.getByRole('heading', { name: 'Ergebnis' }).waitFor();
      await preview.locator('strong', { hasText: 'Wichtig:' }).waitFor();
      await preview.getByRole('button', { name: 'Grundschule am See' }).waitFor();
      assert.equal(await preview.locator('.wikilink.missing').innerText(), 'Gibt es nicht');
      assert.ok((await preview.innerText()).includes('<script>window.__xss = 1</script>'), 'HTML wird als Text angezeigt');
      assert.equal(await page.evaluate(() => window.__xss), undefined, 'Skripte dürfen nicht laufen');
      assert.equal(await preview.locator('script').count(), 0);
      assert.equal(await preview.locator('a[href^="javascript"]').count(), 0, 'gefährliche Links entfallen');
      assert.equal(await preview.getByRole('checkbox').count(), 2);
      assert.equal(await preview.getByRole('checkbox', { name: 'Aufgabe: Termin vereinbart' }).isChecked(), true);
      await shot(page, '13-protokoll-vorschau');

      // Aufgabe in der Vorschau abhaken: der Quelltext ändert sich.
      await preview.getByRole('checkbox', { name: 'Aufgabe: Konzept schicken' }).check();
      await page.getByRole('tab', { name: 'Bearbeiten' }).click();
      assert.match(await page.getByRole('textbox', { name: 'Protokoll (Markdown)' }).inputValue(), /- \[x\] Konzept schicken/);
      await page.getByRole('button', { name: 'Speichern' }).click();

      // Wikilink führt zum Eintrag.
      await page.getByRole('tab', { name: 'Vorschau' }).click();
      await preview.getByRole('button', { name: 'Grundschule am See' }).click();
      await page.getByRole('heading', { name: 'Grundschule am See' }).waitFor();
      await page.locator('.related').getByRole('button', { name: 'Beratung Tablets' }).waitFor();

      // Im Graphen verbindet der Wikilink Protokoll und Schule.
      await nav('Graph');
      await page.getByRole('button', { name: 'Protokoll: Beratung Tablets' }).click();
      const panel = page.getByRole('complementary', { name: 'Auswahl' });
      await panel.getByRole('button', { name: /Grundschule am See/ }).waitFor();
      await panel.getByRole('button', { name: /Tablet-Einsatz im Unterricht/ }).waitFor();
      await shot(page, '14-graph-protokoll');
    });

    await step('Auftragsbuch: Fristen, Filter, Abzeichen, Aufträge aus Protokollen', async () => {
      const isoDay = (offset) => {
        const d = new Date();
        d.setDate(d.getDate() + offset);
        return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
      };
      const nav = (name) => page.getByRole('navigation').getByRole('button', { name, exact: false }).first().click();
      const book = page.getByRole('region', { name: 'Auftragsbuch' });

      await nav('Auftragsbuch');
      const create = async (fields) => {
        await page.getByRole('button', { name: 'Auftrag anlegen' }).click();
        assert.equal(await page.getByLabel('Status *').inputValue(), 'Offen', 'Status ist vorbelegt');
        await page.getByLabel('Titel *').fill(fields.titel);
        if (fields.auftraggeber) await page.getByLabel('Auftraggeber').selectOption(fields.auftraggeber);
        if (fields.frist) await page.getByLabel('Frist', { exact: true }).fill(fields.frist);
        await page.getByRole('button', { name: 'Speichern' }).click();
        await page.getByRole('heading', { name: fields.titel }).waitFor();
      };
      await create({ titel: 'Zweiter Auftrag ohne Frist' });
      await create({ titel: 'Konzept überarbeiten', auftraggeber: 'Sachgebietsleitung', frist: isoDay(3) });
      await create({ titel: 'Bericht an das Ministerium', auftraggeber: 'Ministerium', frist: isoDay(-1) });

      // Nach Frist sortiert, überfällig zuerst, ohne Frist zuletzt.
      const items = await book.locator('li button').allInnerTexts();
      assert.equal(items.length, 3);
      assert.match(items[0], /Bericht an das Ministerium[\s\S]*überfällig seit 1 Tag/);
      assert.match(items[1], /Konzept überarbeiten[\s\S]*fällig in 3 Tagen/);
      assert.match(items[2], /Zweiter Auftrag ohne Frist[\s\S]*ohne Frist/);
      await shot(page, '16-auftragsbuch');
      await page.getByRole('navigation').getByRole('button', { name: /Auftragsbuch \(1 überfällig\)/ }).waitFor();

      // Erledigen: verschwindet aus „Offen“, steht unter „Erledigt“ und das Abzeichen verschwindet.
      await book.getByRole('button', { name: /Bericht an das Ministerium/ }).click();
      await page.getByLabel('Status *').selectOption('Erledigt');
      await page.getByLabel('Erledigt am').fill(isoDay(0));
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Bericht an das Ministerium' }).waitFor();
      // Erledigte Aufträge sind unter „Offen“ ausgeblendet (die Liste lädt nach dem Speichern neu).
      await book.locator('li button', { hasText: 'Bericht an das Ministerium' }).waitFor({ state: 'detached' });
      await page.getByRole('navigation').getByRole('button', { name: /^Auftragsbuch$/ }).waitFor();
      await book.getByRole('button', { name: 'Erledigt', exact: true }).click();
      await book.locator('li button', { hasText: 'Bericht an das Ministerium' }).waitFor();
      assert.match(await book.locator('li button').first().innerText(), /erledigt am/);
      await book.getByRole('button', { name: 'Alle', exact: true }).click();
      assert.equal(await book.locator('li button').count(), 3);
      await book.getByRole('button', { name: 'Offen', exact: true }).click();

      // Aufträge aus dem Protokoll: Schule am Protokoll setzen, offene Aufgaben ergänzen.
      await nav('Protokolle');
      await page.getByRole('button', { name: 'Beratung Tablets' }).click();
      await page.getByLabel('Schule', { exact: true }).selectOption({ label: 'Grundschule am See' });
      const textbox = page.getByRole('textbox', { name: 'Protokoll (Markdown)' });
      await textbox.fill('# Ergebnis\n- [ ] Rückmeldung an die Schule schicken\n- [ ] Fortbildung beantragen\n- [x] Termin vereinbart\n\nHier steht ein Satz, der zum Auftrag werden soll.');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Beratung Tablets' }).waitFor();

      await page.getByRole('tab', { name: 'Vorschau' }).click();
      const preview = page.getByLabel('Protokoll (Markdown): Vorschau');
      // Nur offene und erledigte Aufgaben haben eine Schaltfläche: drei Aufgaben, drei Schaltflächen.
      assert.equal(await preview.getByRole('button', { name: 'Auftrag anlegen' }).count(), 3);
      await preview.locator('li', { hasText: 'Rückmeldung an die Schule schicken' }).getByRole('button', { name: 'Auftrag anlegen' }).click();
      await preview.locator('li', { hasText: 'Rückmeldung an die Schule schicken' }).getByRole('button', { name: 'Auftrag öffnen' }).waitFor();
      assert.equal(await preview.getByRole('button', { name: 'Auftrag anlegen' }).count(), 2, 'ein zweites Anlegen ist nicht möglich');
      await page.locator('.related').getByRole('button', { name: 'Rückmeldung an die Schule schicken' }).waitFor();

      // Der Auftrag kennt Protokoll, Schule und Datum.
      await preview.getByRole('button', { name: 'Auftrag öffnen' }).click();
      await page.getByRole('heading', { name: 'Rückmeldung an die Schule schicken' }).waitFor();
      assert.equal(await page.getByLabel('Status *').inputValue(), 'Offen');
      assert.equal(await page.getByLabel('Erteilt am').inputValue(), '2026-10-30');
      await until(async () => (await selectedText(page.getByLabel('Aus Protokoll'))) === 'Beratung Tablets', 'Auftrag verweist auf das Protokoll');
      await until(async () => (await selectedText(page.getByLabel('Schule', { exact: true }))) === 'Grundschule am See', 'Auftrag übernimmt die Schule');
      await shot(page, '17-auftrag-aus-protokoll');

      // Auftrag aus markiertem Text.
      await nav('Protokolle');
      await page.getByRole('button', { name: 'Beratung Tablets' }).click();
      const box = page.getByRole('textbox', { name: 'Protokoll (Markdown)' });
      const selectionButton = page.getByRole('button', { name: 'Auftrag aus Markierung anlegen' });
      assert.equal(await selectionButton.isDisabled(), true, 'ohne Markierung gesperrt');
      await box.evaluate((el) => {
        const needle = 'ein Satz, der zum Auftrag werden soll';
        const start = el.value.indexOf(needle);
        el.focus();
        el.setSelectionRange(start, start + needle.length);
        el.dispatchEvent(new Event('select', { bubbles: true }));
      });
      await selectionButton.click();
      await page.locator('.related').getByRole('button', { name: 'ein Satz, der zum Auftrag werden soll' }).waitFor();
      const orders = await bridgeCall(bridgeA, 'list', { kind: 'auftrag' });
      assert.ok(orders.some((o) => o.fields.titel === 'ein Satz, der zum Auftrag werden soll'));
      assert.equal(orders.filter((o) => o.fields.protokoll).length, 2);

      // Der Graph verbindet Aufträge mit ihrem Protokoll.
      await nav('Graph');
      await page.getByRole('button', { name: 'Auftrag: Rückmeldung an die Schule schicken' }).click();
      await page.getByRole('complementary', { name: 'Auswahl' }).getByRole('button', { name: /Beratung Tablets/ }).waitFor();
    });

    await step('Protokolltext: gleichzeitige Änderung ergibt eine Konfliktkopie, nichts geht verloren', async () => {
      await bridgeCall(bridgeA, 'sync');
      await bridgeCall(bridgeB, 'sync');
      const list = await bridgeCall(bridgeB, 'list', { kind: 'protokoll' });
      assert.equal(list.length, 1, 'Gerät B hat das Protokoll');
      const id = list[0].id;
      await bridgeCall(bridgeB, 'save', { kind: 'protokoll', id, fields: { text: 'Fassung von Gerät B' } });
      await bridgeCall(bridgeB, 'sync');

      await page.getByRole('navigation').getByRole('button', { name: 'Protokolle', exact: true }).click();
      await page.getByRole('button', { name: 'Beratung Tablets' }).click();
      await page.getByRole('textbox', { name: 'Protokoll (Markdown)' }).fill('Fassung von Gerät A');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Beratung Tablets' }).waitFor();

      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
      await page.getByRole('navigation').getByRole('button', { name: /^Konflikte \(1\)$/ }).waitFor();
      await page.getByRole('navigation').getByRole('button', { name: /^Konflikte/ }).click();
      await page.getByText('Fassung von Gerät A').waitFor();
      await page.getByText('Fassung von Gerät B').waitFor();
      await shot(page, '15-protokoll-konflikt');

      // Den anderen Text übernehmen.
      const current = (await bridgeCall(bridgeA, 'get', { id })).fields.text;
      const other = current === 'Fassung von Gerät A' ? 'Fassung von Gerät B' : 'Fassung von Gerät A';
      await page.getByRole('button', { name: 'Anderen Wert übernehmen' }).click();
      await page.getByText('Keine offenen Konflikte.').waitFor();
      assert.equal((await bridgeCall(bridgeA, 'get', { id })).fields.text, other);
    });

    await step('Dienstreise: Stammdaten, Antrag, Mail, Abrechnung, Anhänge, Status', async () => {
      const smtp = await startSmtp({ user: 'max', pass: 'geheim' });
      const filesDir = tempDir('bdb-e2e-files-');
      const fixtures = join(repo, 'core', 'tests', 'fixtures');
      const nav = (name) => page.getByRole('navigation').getByRole('button', { name, exact: false }).first().click();

      // Ordner für PDFs und Anhänge eintragen.
      await nav('Einstellungen');
      await page.getByLabel('Ordner für PDFs und Anhänge').fill(filesDir);
      await page.getByRole('button', { name: 'Speichern', exact: true }).first().click();
      await page.getByText('Gespeichert.').first().waitFor();

      // Stammdaten mit Unterschrift und (zunächst falschem) Mail-Passwort.
      const st = page.getByRole('form', { name: 'Stammdaten' });
      const fill = async (label, value) => st.getByLabel(label, { exact: true }).fill(value);
      await fill('Name', 'Mustermann');
      await fill('Vorname', 'Max');
      await fill('PLZ', '89073');
      await fill('Ort', 'Ulm');
      await fill('Straße', 'Hauptstraße 1');
      await fill('Beschäftigungsbehörde (Kurzname)', 'Musterschule Ulm');
      await fill('Personalnummer (VIVA)', '9876543');
      await fill('IBAN', 'DE89 3704 0044 0532 0130 00');
      await fill('BIC', 'COBADEFFXXX');
      await fill('Ort für Unterschrift', 'Ulm');
      await fill('Empfänger Dienstreiseantrag (Verwaltung)', 'verwaltung@example.org');
      await fill('Empfänger Abrechnung (LfF)', 'lff@example.org');
      await fill('Absenderadresse', 'max.mustermann@example.org');
      await fill('SMTP-Server', '127.0.0.1');
      await fill('SMTP-Port', String(smtp.port));
      await fill('SMTP-Benutzer', 'max');
      await st.getByLabel('Verschlüsselung', { exact: true }).selectOption('keine');
      await st.getByLabel('SMTP-Passwort').fill('falsches-passwort');
      await st.locator('#svg-upload').setInputFiles(join(fixtures, 'unterschrift.svg'));
      await st.getByAltText('Vorschau der Unterschrift').waitFor();
      await st.getByRole('button', { name: 'Stammdaten speichern' }).click();
      await st.getByText('Gespeichert.').waitFor();
      await shot(page, '18-stammdaten');

      // Reise anlegen: falsche Reihenfolge der Zeiten wird abgelehnt.
      await nav('Dienstreisen');
      await page.getByRole('button', { name: 'Dienstreise anlegen' }).click();
      await page.getByLabel('Zweck / Titel *').fill('Beratung Medienkonzept');
      await page.getByLabel('Geschäftsort *').fill('Dillingen a. d. Donau');
      await page.getByLabel('Reiseantritt *').fill('2026-11-16T06:45');
      await page.getByLabel('Beginn Dienstgeschäft *').fill('2026-11-16T10:00');
      await page.getByLabel('Ende Dienstgeschäft *').fill('2026-11-16T08:00');
      await page.getByLabel('Beendigung der Reise *').fill('2026-11-17T18:20');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('alert').filter({ hasText: 'darf nicht nach' }).waitFor();
      await page.getByLabel('Ende Dienstgeschäft *').fill('2026-11-17T15:30');
      await page.getByLabel('Deutsche Bahn').check();
      await page.getByLabel('BahnCard vorhanden').check();
      await page.getByLabel('Fahrkartenpreis (€)').fill('123,45');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'Beratung Medienkonzept' }).waitFor();
      const list = page.getByRole('region', { name: 'Dienstreisen' });
      await list.locator('li button', { hasText: '16.11.2026 · Entwurf' }).waitFor();

      // Antrag erzeugen.
      const panel = page.getByRole('region', { name: 'Dokumente der Reise' });
      await panel.getByRole('button', { name: 'Antrag erzeugen' }).click();
      await panel.getByText('Antrag erzeugt.').waitFor();
      await panel.getByTestId('antrag-datei').filter({ hasText: 'antrag-2026-11-16.pdf' }).waitFor();
      const tripId = (await bridgeCall(bridgeA, 'list', { kind: 'reise' }))[0].id;
      const readPdf = async (path) => Buffer.from((await bridgeCall(bridgeA, 'file_read', { path })).data, 'base64');
      const trip = await bridgeCall(bridgeA, 'get', { id: tripId });
      const antragPdf = await readPdf(trip.fields.antrag_pdf);
      assert.equal(antragPdf.subarray(0, 4).toString(), '%PDF');
      const antragText = antragPdf.toString('latin1');
      for (const wanted of ['(Mustermann)', '(Beratung Medienkonzept)', '(16.11.2026)', '(06:45)']) {
        assert.ok(antragText.includes(wanted), `Antrag enthält ${wanted}`);
      }
      assert.ok(antragText.includes(' cm\n') && /\d+ \d+ c\n/.test(antragText), 'Unterschrift wurde auf die Seite gezeichnet');

      // Mail mit falschem Passwort scheitert und ändert nichts.
      await panel.getByRole('button', { name: 'Antrag per Mail senden …' }).click();
      const mail = page.getByRole('region', { name: 'Mail', exact: true });
      await mail.getByTestId('mail-to').filter({ hasText: 'verwaltung@example.org' }).waitFor();
      assert.equal(await mail.getByTestId('mail-from').innerText(), 'max.mustermann@example.org');
      assert.equal(await mail.getByLabel('Betreff').inputValue(), 'Antrag auf Genehmigung einer Dienstreise am 16.11.2026 bis 17.11.2026');
      await shot(page, '19-mail');
      await mail.getByRole('button', { name: 'Senden' }).click();
      await mail.getByRole('alert').filter({ hasText: 'Versand fehlgeschlagen' }).waitFor();
      assert.equal(smtp.received.length, 0);
      await mail.getByRole('button', { name: 'Abbrechen' }).click();
      await list.locator('li button', { hasText: 'Entwurf' }).waitFor();

      // Passwort korrigieren, dann senden.
      await nav('Einstellungen');
      await st.getByLabel('SMTP-Passwort').fill('geheim');
      await st.getByRole('button', { name: 'Stammdaten speichern' }).click();
      await st.getByText('Gespeichert.').waitFor();
      await nav('Dienstreisen');
      await list.getByRole('button', { name: /Beratung Medienkonzept/ }).click();
      await panel.getByRole('button', { name: 'Antrag per Mail senden …' }).click();
      await mail.getByRole('button', { name: 'Senden' }).click();
      await panel.getByText('Antrag gesendet.').waitFor();
      assert.equal(smtp.received.length, 1);
      const sent = await simpleParser(smtp.received[0].raw);
      assert.equal(sent.subject, 'Antrag auf Genehmigung einer Dienstreise am 16.11.2026 bis 17.11.2026');
      assert.equal(sent.to.value[0].address, 'verwaltung@example.org');
      assert.equal(sent.attachments.length, 1);
      assert.equal(sent.attachments[0].filename, 'antrag-2026-11-16.pdf');
      assert.deepEqual(sent.attachments[0].content, antragPdf, 'die gesendete PDF ist die erzeugte');
      assert.ok(smtp.received[0].authed);
      await list.locator('li button', { hasText: '16.11.2026 · Beantragt' }).waitFor();

      // Abrechnung: Formular des Veranstalters, Genehmigung und Anhänge.
      await panel.getByLabel('Abrechnungsformular des Veranstalters (ausfüllbares PDF)').setInputFiles(join(fixtures, 'fibs_form.pdf'));
      await panel.getByText('Formular gespeichert.').waitFor();
      await panel.getByLabel('Genehmigung hinzufügen (PDF)').setInputFiles({ name: 'Genehmigung.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 genehmigt') });
      await panel.getByText('Genehmigung gespeichert.').waitFor();
      await list.locator('li button', { hasText: '16.11.2026 · Genehmigt' }).waitFor();
      // Das Formular folgt dem neuen Status, sonst würde ein späteres Speichern ihn zurückschreiben.
      await until(async () => (await page.getByLabel('Status *').inputValue()) === 'Genehmigt', 'Formular zeigt Status Genehmigt');
      await panel.getByLabel('Art', { exact: true }).selectOption('Fahrkarte');
      await panel.getByLabel('Anhang hinzufügen').setInputFiles({ name: 'ticket.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 ticket') });
      await panel.getByText('Anhang gespeichert.').waitFor();
      await panel.getByLabel('Art', { exact: true }).selectOption('Hotelrechnung');
      await panel.getByLabel('Anhang hinzufügen').setInputFiles({ name: 'hotel.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 hotel') });
      await panel.getByText('Anhang gespeichert.').waitFor();
      await panel.getByRole('checkbox', { name: 'Hotelrechnung: hotel.pdf' }).uncheck();
      await panel.getByRole('button', { name: 'Abrechnung erzeugen' }).click();
      await panel.getByText('Abrechnung erzeugt.').waitFor();
      const settled = await bridgeCall(bridgeA, 'get', { id: tripId });
      const abrechnungText = (await readPdf(settled.fields.abrechnung_pdf)).toString('latin1');
      for (const wanted of ['(AZ-2026/1234)', '(123,45)', '(DE89)', '(COBADEFFXXX)', '(16.11.2026 06:45)']) {
        assert.ok(abrechnungText.includes(wanted), `Abrechnung enthält ${wanted}`);
      }

      await panel.getByRole('button', { name: 'Abrechnung per Mail senden …' }).click();
      await mail.getByLabel('Betreff').waitFor();
      assert.equal(await mail.getByRole('checkbox', { name: /Hotelrechnung/ }).isChecked(), false, 'abgewählter Anhang ist nicht vorgewählt');
      assert.equal(await mail.getByRole('checkbox', { name: /Reisekostenabrechnung/ }).isDisabled(), true, 'die Abrechnung ist immer dabei');
      await shot(page, '20-mail-abrechnung');
      await mail.getByRole('button', { name: 'Senden' }).click();
      await panel.getByText('Abrechnung gesendet.').waitFor();
      assert.equal(smtp.received.length, 2);
      const second = await simpleParser(smtp.received[1].raw);
      assert.equal(second.to.value[0].address, 'lff@example.org');
      assert.equal(second.subject, 'Reisekostenabrechnung Beratung Medienkonzept (16.11.2026 bis 17.11.2026)');
      const names = second.attachments.map((a) => a.filename).sort();
      assert.deepEqual(names.map((n) => n.replace(/-\d{4}-\d{2}-\d{2}/, '')), ['abrechnung.pdf', 'genehmigung-Genehmigung.pdf', 'anhang-ticket.pdf'].sort());
      await list.locator('li button', { hasText: '16.11.2026 · Abgerechnet' }).waitFor();
      await until(async () => (await page.getByLabel('Status *').inputValue()) === 'Abgerechnet', 'Formular zeigt Status Abgerechnet');
      // Speichern ohne eigene Änderung an einem anderen Feld lässt den Status unverändert.
      await page.getByLabel('Aktenzeichen').fill('AZ-1');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await until(async () => (await bridgeCall(bridgeA, 'get', { id: tripId })).fields.aktenzeichen === 'AZ-1', 'Aktenzeichen gespeichert');
      assert.equal((await bridgeCall(bridgeA, 'get', { id: tripId })).fields.status, 'Abgerechnet', 'der Status bleibt');

      // Bezahlt eintragen und nach Status filtern.
      await page.getByLabel('Status *').selectOption('Bezahlt');
      await page.getByLabel('Bezahlt am').fill('2026-12-01');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await list.locator('li button', { hasText: '16.11.2026 · Bezahlt' }).waitFor();
      await list.getByLabel('Status filtern').selectOption('Beantragt');
      await page.getByText('Keine Treffer.').waitFor();
      await list.getByLabel('Status filtern').selectOption('Bezahlt');
      await list.locator('li button', { hasText: 'Bezahlt' }).waitFor();
      await shot(page, '21-reise-bezahlt');

      // Die Dateien liegen im Ordner der Reise, auch das Formular des Veranstalters.
      const tripDir = join(filesDir, 'Dienstreisen');
      const [folder] = readdirSync(tripDir);
      assert.match(folder, /^2026-11-16-beratung-medienkonzept-/);
      const files = readdirSync(join(tripDir, folder)).sort();
      assert.deepEqual(
        files.map((f) => f.replace(/-\d{4}-\d{2}-\d{2}/, '')),
        ['abrechnung.pdf', 'anhang-hotel.pdf', 'anhang-ticket.pdf', 'antrag.pdf', 'genehmigung-Genehmigung.pdf', 'veranstalter-fibs_form.pdf'],
      );
      assert.deepEqual(readFileSync(join(tripDir, folder, 'veranstalter-fibs_form.pdf')), readFileSync(join(fixtures, 'fibs_form.pdf')));
      smtp.close();
    });

    await step('Fortbildung: Bedarfe sammeln, planen, ausschreiben, durchführen, nachbereiten', async () => {
      const nav = (name) => page.getByRole('navigation').getByRole('button', { name, exact: false }).first().click();

      // Bedarfsanmeldungen: eine über die Oberfläche, zwei über den Kern.
      await nav('Bedarfsanmeldungen');
      await page.getByRole('button', { name: 'Bedarfsanmeldung anlegen' }).click();
      assert.equal(await page.getByLabel('Status *').inputValue(), 'Offen', 'Status ist vorbelegt');
      await page.getByLabel('Thema *').fill('KI im Unterricht');
      await page.getByLabel('Anzahl Interessenten').fill('8');
      await page.getByLabel('Eingegangen am').fill('2026-10-01');
      await page.getByLabel('Schule', { exact: true }).selectOption({ label: 'Grundschule am See' });
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'KI im Unterricht' }).waitFor();
      await bridgeCall(bridgeA, 'save', { kind: 'bedarf', fields: { thema: 'Tablets im Unterricht', interessenten: 12, eingegangen_am: '2026-10-10' } });
      await bridgeCall(bridgeA, 'save', { kind: 'bedarf', fields: { thema: 'Datenschutz in der Schule', interessenten: 5, eingegangen_am: '2026-10-20' } });
      await nav('Dienstreisen');
      await page.getByRole('navigation').getByRole('button', { name: /Bedarfsanmeldungen \(3 offen\)/ }).waitFor();

      // Fortbildung anlegen: sie beginnt in der Planung.
      await nav('Fortbildungen');
      await page.getByRole('button', { name: 'Fortbildung anlegen' }).click();
      await page.getByLabel('Titel *').fill('KI im Unterricht, Tagesveranstaltung');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('heading', { name: 'KI im Unterricht, Tagesveranstaltung' }).waitFor();
      assert.equal(await page.getByLabel('Phase *').inputValue(), 'Planung');
      const process = page.getByRole('region', { name: 'Prozess der Fortbildung' });
      const current = () => process.locator('.phases li[aria-current="step"]').innerText();
      const forward = async (phase) => process.getByRole('button', { name: new RegExp(`Weiter zu „${phase}“`) }).click();

      // Ohne Planungsangaben keine Ausschreibung, die Meldung nennt das Fehlende.
      await forward('Ausschreibung');
      const alert = process.getByRole('alert');
      await alert.waitFor();
      for (const missing of ['Referent', 'Inhalt', 'Ort', 'Anzahl Teilnehmer']) assert.match(await alert.innerText(), new RegExp(missing));
      assert.equal(await current(), 'Planung');

      // Planung ausfüllen und weiter.
      await page.getByLabel('Referent', { exact: true }).fill('Dr. Beispiel');
      await page.getByRole('textbox', { name: 'Inhalt', exact: true }).fill('Grundlagen und Praxisbeispiele');
      await page.getByLabel('Beginn', { exact: true }).fill('2027-02-10T09:00');
      await page.getByLabel('Ort', { exact: true }).fill('Ulm');
      await page.getByLabel('Anzahl Teilnehmer (geplant)').fill('25');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await page.getByRole('region', { name: 'Fortbildungen' }).locator('li button', { hasText: '10.02.2027 · Planung' }).waitFor();
      await forward('Ausschreibung');
      await until(async () => (await current()) === 'Ausschreibung', 'Phase Ausschreibung');
      await page.getByRole('region', { name: 'Fortbildungen' }).locator('li button', { hasText: '10.02.2027 · Ausschreibung' }).waitFor();
      assert.equal(await page.getByLabel('Phase *').inputValue(), 'Ausschreibung', 'das Formular folgt der Phase');

      // Bedarfsanmeldungen zuordnen.
      await process.getByRole('group', { name: 'Offene Anmeldungen zuordnen' }).getByLabel('KI im Unterricht').check();
      await process.getByRole('group', { name: 'Offene Anmeldungen zuordnen' }).getByLabel('Tablets im Unterricht').check();
      await process.getByRole('button', { name: 'Zuordnen' }).click();
      await process.getByTestId('bedarf-summe').filter({ hasText: '2 zugeordnet, 20 Interessenten (geplant: 25 Teilnehmer)' }).waitFor();
      const requests = await bridgeCall(bridgeA, 'list', { kind: 'bedarf' });
      assert.deepEqual(requests.map((r) => r.fields.status).sort(), ['Eingeplant', 'Eingeplant', 'Offen']);
      await page.getByRole('navigation').getByRole('button', { name: /Bedarfsanmeldungen \(1 offen\)/ }).waitFor();
      await shot(page, '22-fortbildung');

      // Durchführung braucht die FIBS-Nummer.
      await forward('Durchführung');
      await alert.filter({ hasText: 'FIBS' }).waitFor();
      await page.getByLabel('Veranstaltungsnummer in FIBS').fill('2027-12345');
      await page.getByRole('button', { name: 'Speichern' }).click();
      await until(async () => (await bridgeCall(bridgeA, 'list', { kind: 'fortbildung' }))[0].fields.fibs_nummer === '2027-12345', 'FIBS-Nummer gespeichert');
      await forward('Durchführung');
      await until(async () => (await current()) === 'Durchführung', 'Phase Durchführung');

      // Standard-Checklisten.
      const progressOf = (field) => process.getByTestId(`fortschritt-${field}`);
      await process.locator('li', { hasText: 'Durchführung:' }).getByRole('button', { name: 'Standard-Checkliste einfügen' }).click();
      await progressOf('checkliste_durchfuehrung').filter({ hasText: '0 von 6 Aufgaben erledigt' }).waitFor();
      await forward('Nachbereitung');
      await until(async () => (await current()) === 'Nachbereitung', 'Phase Nachbereitung');
      await process.locator('li', { hasText: 'Nachbereitung:' }).getByRole('button', { name: 'Standard-Checkliste einfügen' }).click();
      await progressOf('checkliste_nachbereitung').filter({ hasText: '0 von 6 Aufgaben erledigt' }).waitFor();

      // Abschluss erst mit abgehakter Checkliste. Zwei Aufgaben abhaken: es bleiben vier offene.
      const trainingId = (await bridgeCall(bridgeA, 'list', { kind: 'fortbildung' }))[0].id;
      const checklist = (await bridgeCall(bridgeA, 'get', { id: trainingId })).fields.checkliste_nachbereitung;
      const lines = checklist.split('\n');
      lines[0] = lines[0].replace('[ ]', '[x]');
      lines[1] = lines[1].replace('[ ]', '[x]');
      await bridgeCall(bridgeA, 'save', { kind: 'fortbildung', id: trainingId, fields: { checkliste_nachbereitung: lines.join('\n') } });
      await forward('Abgeschlossen');
      await alert.filter({ hasText: '4 offene Aufgaben' }).waitFor();
      assert.equal(await current(), 'Nachbereitung');
      await bridgeCall(bridgeA, 'save', { kind: 'fortbildung', id: trainingId, fields: { checkliste_nachbereitung: checklist.replaceAll('[ ]', '[x]') } });
      await forward('Abgeschlossen');
      await until(async () => (await current()) === 'Abgeschlossen', 'Phase Abgeschlossen');
      assert.equal(await process.getByRole('button', { name: 'Abgeschlossen', exact: true }).isDisabled(), true);
      await progressOf('checkliste_nachbereitung').filter({ hasText: '6 von 6 Aufgaben erledigt' }).waitFor();

      // Zurück geht immer.
      await process.getByRole('button', { name: /Zurück zu „Nachbereitung“/ }).click();
      await until(async () => (await current()) === 'Nachbereitung', 'zurück zur Nachbereitung');
      await shot(page, '23-fortbildung-nachbereitung');
    });

    await step('Dienstreise: Fahrkartenpreis erkennen und Erstattungsantrag R002', async () => {
      const fixtures = join(repo, 'core', 'tests', 'fixtures');
      const panel = page.getByRole('region', { name: 'Dokumente der Reise' });
      await page.getByRole('navigation').getByRole('button', { name: 'Dienstreisen' }).click();
      await page.getByRole('region', { name: 'Dienstreisen' }).getByRole('button', { name: /Beratung Medienkonzept/ }).click();

      // Fahrkarte als PDF mit Text anhängen. Die frühere „Fahrkarte“ ist kein lesbares PDF.
      await panel.getByLabel('Art', { exact: true }).selectOption('Fahrkarte');
      await panel.getByLabel('Anhang hinzufügen').setInputFiles(join(fixtures, 'ticket_sample.pdf'));
      await panel.getByText('Anhang gespeichert.').waitFor();
      await panel.getByRole('button', { name: 'Preis aus Fahrkarten erkennen' }).click();
      const found = panel.getByRole('list', { name: 'Erkannte Preise' });
      await found.locator('li', { hasText: 'ticket_sample.pdf' }).getByText('29,85 €').waitFor();
      await found.locator('li', { hasText: 'ticket.pdf' }).getByText('kein Preis erkannt').waitFor();
      await shot(page, '25-ticketpreis');
      await found.locator('li', { hasText: 'ticket_sample.pdf' }).getByRole('button', { name: 'Als Fahrkartenpreis übernehmen' }).click();
      await panel.getByText('Fahrkartenpreis übernommen.').waitFor();
      const tripId = (await bridgeCall(bridgeA, 'list', { kind: 'reise' }))[0].id;
      assert.equal((await bridgeCall(bridgeA, 'get', { id: tripId })).fields.fahrkartenpreis, 29.85);
      // Das Formular zeigt den neuen Preis.
      await until(async () => (await page.getByLabel('Fahrkartenpreis (€)').inputValue()) === '29.85', 'Formular zeigt den Preis');

      // Erstattungsantrag R002 ohne Veranstalterformular.
      await panel.getByRole('button', { name: 'Erstattungsantrag R002 (LfF) vorausfüllen' }).click();
      await panel.getByText('Erstattungsantrag R002 erzeugt.').waitFor();
      const trip = await bridgeCall(bridgeA, 'get', { id: tripId });
      const pdf = Buffer.from((await bridgeCall(bridgeA, 'file_read', { path: trip.fields.abrechnung_pdf })).data, 'base64').toString('latin1');
      for (const wanted of ['(Mustermann)', '(29,85)', '(DE89370400440532013000)', '(Beratung Medienkonzept)', '(16.11.2026)']) {
        assert.ok(pdf.includes(wanted), `R002 enthält ${wanted}`);
      }
      assert.ok(pdf.includes('S1 Name 1'), 'es ist das R002-Formular');
    });

    await step('Tätigkeitsbericht: Zeiträume, Zahlen aus allen Modulen, Speichern', async () => {
      const nav = (name) => page.getByRole('navigation').getByRole('button', { name, exact: false }).first().click();
      await nav('Tätigkeitsbericht');
      const report = page.getByTestId('report');
      await report.getByRole('heading', { name: 'Beratungen' }).waitFor();

      // Kalenderjahr 2026: zwei Beratungen (90 Minuten), eine Dienstreise, ein Protokoll.
      await page.getByLabel('Von').fill('2026-01-01');
      await page.getByLabel('Bis').fill('2026-12-31');
      const text = async () => report.innerText();
      await until(async () => (await text()).includes('Zeitraum: 01.01.2026 bis 31.12.2026') && (await text()).includes('Anzahl: 2'), 'Bericht mit Beratungen angezeigt');
      let body = await text();
      assert.match(body, /Anzahl: 2/);
      assert.match(body, /Beratungszeit: 1 Std\. 30 Min\./);
      assert.match(body, /nach Art: .*Vor Ort 1 \(1 Std\. 30 Min\.\)/);
      assert.match(body, /16\.11\.2026: Beratung Medienkonzept, Dillingen a\. d\. Donau \(Bezahlt\)/);
      assert.match(body, /Keine Fortbildungen im Zeitraum/);
      assert.match(body, /Eingegangen: 3 \(eingeplant 2, offen 1\)/);
      await shot(page, '24-bericht');

      // 2027: die Fortbildung (Nachbereitung) zählt als durchgeführt, mit geplanter Teilnehmerzahl.
      await page.getByLabel('Von').fill('2027-01-01');
      await page.getByLabel('Bis').fill('2027-12-31');
      await until(async () => (await text()).includes('KI im Unterricht, Tagesveranstaltung'), 'Fortbildung im Bericht');
      body = await text();
      assert.match(body, /Durchgeführt: 1 \(Teilnehmende insgesamt: 25\)/);
      assert.match(body, /10\.02\.2027: KI im Unterricht, Tagesveranstaltung, Ulm, 25 Teilnehmende/);
      assert.match(body, /Keine Beratungen im Zeitraum/);

      // Der Bericht enthält keine Personen.
      for (const secret of ['Weber', 'Bauer', 'Müller', 'Mustermann']) assert.ok(!body.includes(secret), `${secret} im Bericht`);

      // Ungültiger Zeitraum.
      await page.getByLabel('Bis').fill('2026-01-01');
      await page.getByRole('alert').filter({ hasText: 'gültigen Zeitraum' }).waitFor();
      assert.equal(await page.getByRole('button', { name: 'Als Datei speichern' }).isDisabled(), true);

      // Voreinstellung: Schuljahr, dann speichern.
      await page.getByRole('button', { name: /^Kalenderjahr 2026$/ }).click();
      await page.getByRole('button', { name: 'Als Datei speichern' }).click();
      const saved = page.getByRole('status').filter({ hasText: 'Gespeichert: Berichte/Tätigkeitsbericht 01.01.2026 bis 31.12.2026.md' });
      await saved.waitFor();
      const content = Buffer.from((await bridgeCall(bridgeA, 'file_read', { path: 'Berichte/Tätigkeitsbericht 01.01.2026 bis 31.12.2026.md' })).data, 'base64').toString();
      assert.match(content, /^# Tätigkeitsbericht\n/);
      assert.match(content, /- Beratungszeit: 1 Std\. 30 Min\./);
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

    await step('Import aus dem alten Dienstreisebuster', async () => {
      const old = join(repo, 'core', 'tests', 'fixtures', 'django');
      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      const form = page.getByRole('form', { name: 'Import aus dem alten Dienstreisebuster' });
      const importButton = form.getByRole('button', { name: 'Importieren' });
      assert.equal(await importButton.isDisabled(), true, 'ohne Pfade gesperrt');
      await form.getByLabel(/Datenbank des alten Tools/).fill(join(old, 'db.sqlite3'));
      await form.getByLabel(/Ordner mit den Dateien/).fill(join(old, 'media'));
      await importButton.click();
      const report = form.getByTestId('import-report');
      // Die Stammdaten sind schon ausgefüllt und bleiben unverändert, die beiden Reisen kommen dazu.
      await report.getByText(/Stammdaten: übersprungen\. Reisen: 2 importiert, 0 übersprungen\. Anhänge: 2 importiert, 0 übersprungen\. Dateien kopiert: 6/).waitFor();
      await report.getByText(/Die Stammdaten in der App sind schon ausgefüllt/).waitFor();
      await shot(page, '26-import');

      // Zweiter Durchlauf: nichts Neues.
      await importButton.click();
      await report.getByText(/Reisen: 0 importiert, 2 übersprungen\. Anhänge: 0 importiert, 2 übersprungen/).waitFor();

      // Die importierten Reisen sind da, mit Ortszeit, Status und Dateien.
      await page.getByRole('navigation').getByRole('button', { name: 'Dienstreisen' }).click();
      const list = page.getByRole('region', { name: 'Dienstreisen' });
      await list.locator('li button', { hasText: 'Beratung vor Ort' }).getByText('14.07.2026 · Entwurf').waitFor();
      await list.locator('li button', { hasText: 'Fortbildung Künstliche Intelligenz' }).getByText('16.11.2026 · Bezahlt').waitFor();
      await list.getByRole('button', { name: /Fortbildung Künstliche Intelligenz/ }).click();
      assert.equal(await page.getByLabel('Reiseantritt *').inputValue(), '2026-11-16T06:45', 'Winterzeit: 05:45 UTC sind 06:45');
      const panel = page.getByRole('region', { name: 'Dokumente der Reise' });
      await panel.getByTestId('antrag-datei').filter({ hasText: 'antrag-2026-11-16.pdf' }).waitFor();
      await panel.getByTestId('abrechnung-datei').filter({ hasText: 'abrechnung-2026-11-16.pdf' }).waitFor();
      await panel.getByText('fibs_form.pdf').waitFor();
      await panel.getByText('Fahrkarte: ticket.pdf').waitFor();
      assert.equal(await panel.getByRole('checkbox', { name: /Hotelrechnung: hotel_rechnung\.pdf/ }).isChecked(), false);
    });

    await step('Server neu befüllen: leerer Server, beide Rechner kommen zum gleichen Stand', async () => {
      await page.getByRole('navigation').getByRole('button', { name: 'Einstellungen' }).click();
      const syncA = async () => {
        await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
        await page.getByTestId('sync-status').filter({ hasText: 'Abgeglichen' }).waitFor();
      };
      await syncA();
      await bridgeCall(bridgeB, 'sync');
      await syncA();
      const byId = (list) => [...list].sort((x, y) => x.id.localeCompare(y.id));
      const before = byId(await bridgeCall(bridgeA, 'list_all'));
      assert.deepEqual(byId(await bridgeCall(bridgeB, 'list_all')), before, 'vorher gleicher Stand');
      assert.ok(before.length > 5, 'genug Einträge für den Test');

      // Der Server verliert seinen Bestand (neu aufgesetzt). Die Oberfläche bietet das Neubefüllen an.
      await stopServer();
      await startServer();
      const region = page.getByRole('region', { name: 'Server neu befüllen' });
      assert.equal(await region.count(), 0, 'ohne Anlass kein Angebot');
      await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
      await page.getByRole('alert').filter({ hasText: 'Der Server ist leer' }).waitFor();
      await region.waitFor();
      await page.getByTestId('sync-status').filter({ hasText: 'Abgleich fehlgeschlagen' }).waitFor();

      // Erst nach Bestätigung, Abbrechen ändert nichts.
      await region.getByRole('button', { name: 'Server neu befüllen …' }).click();
      await region.getByRole('button', { name: 'Abbrechen' }).click();
      assert.equal((await bridgeRaw(bridgeB, 'sync')).error?.code, 'server_reset', 'B erkennt den leeren Server');
      await shot(page, '27-server-neu-befuellen');
      await region.getByRole('button', { name: 'Server neu befüllen …' }).click();
      await region.getByRole('button', { name: 'Ja, Server neu befüllen' }).click();
      await page.getByText('Der Server ist wieder in Betrieb').waitFor();
      await region.waitFor({ state: 'detached' });
      await page.getByTestId('sync-status').filter({ hasText: 'Abgeglichen' }).waitFor();

      // B merkt, dass A neu befüllt hat, und schließt sich an. Gleiche Werte ergeben keine Konflikte.
      const refilled = await bridgeRaw(bridgeB, 'sync');
      assert.equal(refilled.error?.code, 'server_reset');
      assert.match(refilled.error.message, /anderer Rechner hat den Server neu befüllt/);
      const report = await bridgeCall(bridgeB, 'server_refill');
      assert.equal(report.conflicts, 0, JSON.stringify(report));
      await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
      await page.getByTestId('sync-status').filter({ hasText: 'Abgeglichen' }).waitFor();
      assert.deepEqual(byId(await bridgeCall(bridgeA, 'list_all')), before, 'A unverändert');
      assert.deepEqual(byId(await bridgeCall(bridgeB, 'list_all')), before, 'B gleicher Stand');

      // Danach geht es normal weiter.
      await bridgeCall(bridgeB, 'save', { kind: 'schule', fields: { name: 'Nach dem Neubefüllen' } });
      await bridgeCall(bridgeB, 'sync');
      await page.getByRole('button', { name: 'Jetzt abgleichen' }).click();
      await page.getByTestId('sync-status').filter({ hasText: 'Abgeglichen' }).waitFor();
      assert.ok((await bridgeCall(bridgeA, 'list', { kind: 'schule' })).some((r) => r.fields.name === 'Nach dem Neubefüllen'));
    });

    await step('Keine unerwarteten Fehler in der Browser-Konsole', async () => {
      // Die Brücke antwortet auf Fehler mit HTTP 400, die der Test absichtlich auslöst (falsches Passwort,
      // fehlende Pflichtfelder, ungültiger Recovery-Key, falsche Reihenfolge der Reisezeiten, Mailversand mit
      // falschem Passwort, gesperrte Phasenwechsel der Fortbildung, Abgleich mit leerem Server). Der Browser
      // protokolliert sie als Ladefehler.
      const expected = /^Failed to load resource: the server responded with a status of 400/;
      const unexpected = consoleErrors.filter((m) => !expected.test(m));
      assert.deepEqual(unexpected, [], unexpected.join('\n'));
      const provoked = consoleErrors.length - unexpected.length;
      assert.equal(provoked, 12, `${provoked} statt 12 erwarteter Fehlerantworten`);
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
