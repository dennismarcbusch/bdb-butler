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
      const titles = await page.getByRole('region', { name: 'Beratungen' }).locator('li button').allInnerTexts();
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
      assert.equal(await page.getByLabel('Aus Protokoll').evaluate((el) => el.selectedOptions[0].textContent), 'Beratung Tablets');
      assert.equal(await page.getByLabel('Schule', { exact: true }).evaluate((el) => el.selectedOptions[0].textContent), 'Grundschule am See');
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
      // fehlende Pflichtfelder, ungültiger Recovery-Key). Der Browser protokolliert sie als Ladefehler.
      const expected = /^Failed to load resource: the server responded with a status of 400/;
      const unexpected = consoleErrors.filter((m) => !expected.test(m));
      assert.deepEqual(unexpected, [], unexpected.join('\n'));
      const provoked = consoleErrors.length - unexpected.length;
      assert.equal(provoked, 6, `${provoked} statt 6 erwarteter Fehlerantworten`);
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
