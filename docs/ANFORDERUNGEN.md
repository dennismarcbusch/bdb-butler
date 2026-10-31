# Anforderungen BdB-Butler

Stand: 2026-10-31. Quelle der Wahrheit für Entscheidungen ist diese Datei zusammen mit `ARCHITEKTUR.md`.
Die Projektnotiz im Brain (`/vault/10-Projekte/BdB-Butler.md`) spiegelt denselben Stand.

## Rahmen

- Anwendung für die mBdB-Tätigkeit, genau 1 Nutzer.
- Nutzbar unterwegs und auf wechselnden Dienstrechnern (Windows und Linux, selbst verwaltet), auch offline.
- Zusätzlich Zugang per Android-Smartphone.
- Rechtsgrundlage der Verarbeitung: Art. 4 BayDSG. Die Datenschutzfreigabe liegt vor.
- Hosting des Sync-Servers: LXC-Container auf eigenem Proxmox, hinter zentralem Caddy-Server als Reverse-Proxy auf separatem LXC.

## Datenschutz und Sicherheit

- Alle Daten werden Ende-zu-Ende verschlüsselt. Der Server speichert und sieht nur Chiffretext, auch nicht vorübergehend Klartext.
- Ausgenommen sind:
  - die Mail-Zugangsdaten (liegen im Schlüsselbund des Betriebssystems),
  - PDFs und Anhänge (liegen als Dateien in einem lokalen Ordner, der per Nextcloud oder ein anderes Tool synchronisiert wird; je Rechner wird nur der Ordnerpfad in der App angegeben).
- Entsperren per Master-Passwort mit Recovery-Key. Passkey/YubiKey ist keine Anforderung.
- Das Gerät, auf dem die App läuft, ist verschlüsselt (Festplattenverschlüsselung).

## KI-Anbindung

- Ein lokaler MCP-Server in der App ist von Anfang an vorgesehen. Per KI sollen Daten angelegt, abgefragt und bearbeitet werden können.

## Funktionsbereiche

### Dienstreisemodul

- Dienstreiseantrag (R001) und Abrechnung (FIBS-Format) als PDF mit Unterschrift.
- Mailversand durch den Client: Antrag an die Verwaltung, Abrechnung an das LfF mit auswählbaren Anhängen.
- Statusverfolgung je Reise: Entwurf, Beantragt, Genehmigt, Abgerechnet, Bezahlt.
- Nur Einzelabrechnung. Die Sammelabrechnung (LfF R003) bleibt bewusst außen vor.
- Bestand: Das bisherige Tool „Dienstreisebuster“ (`dienstreisebuster/`, Django). Der Name entfällt, es wird ein Modul des BdB-Butlers.
- Offen: Ticketpreis-Erkennung, R002-Abrechnung (LfF) für Reisen ohne Akademie-Formular, Zuordnung unbekannter Veranstalter-Formulare.

### Schulen und Ansprechpartner

- Verwaltung der Schulen und ihrer Ansprechpartner.
- Graph-Ansicht der Kontaktpersonenverwaltung ähnlich wie in SilverBullet: Kontaktpersonen und ihre Verknüpfungen, zum Beispiel zu Schulen, als Graph.

### Beratungen und Protokolle

- Beratungen mit beratenen Personen.
- Protokolle von Beratungen, Jour Fixes und Dienstbesprechungen im Markdown-Format.

### Auftragsbuch

- Aufträge vom Ministerium oder vom Sachgebietsleiter mit Termin- und Statusverfolgung.
- Aufträge können direkt aus Protokollen erzeugt werden.

### Fortbildungen

- Sammlung von Bedarfsanmeldungen.
- Prozess:
  1. Bedarf
  2. Planung (Referent, Inhalt, Datum und Uhrzeit, Ort, Anzahl Teilnehmer)
  3. Ausschreibung in FIBS
  4. Durchführung (mit Checkliste)
  5. Nachbereitung (Teilnahmebescheinigungen versenden, Abrechnungen versenden etc.)

### Tätigkeitsbericht

- Bericht über die Tätigkeit, gespeist aus den Daten der anderen Module.

## Umfang V1 (Vorschlag, noch nicht bestätigt)

1. Meilenstein 1: Fundament (Krypto-Kern, lokale Datenbank, Sync, App-Gerüst, lokaler MCP-Server) und Modul Schulen/Ansprechpartner mit Graph-Ansicht.
2. Meilenstein 2: Beratungen, Protokolle, Auftragsbuch.
3. Meilenstein 3: Dienstreisemodul (Neuumsetzung; danach wird das Django-Tool abgeschaltet).
4. Nach V1: Fortbildungsmodul, Tätigkeitsbericht, Android-Zugang, Ticketpreis-Erkennung, R002-Abrechnung.

## Offene Punkte

- V1-Umfang und Reihenfolge bestätigen.
- Soll der Sync-Server in Go (entschieden) oder in Rust gebaut werden? Rust hätte gemeinsamen Protokoll- und Krypto-Code mit dem Client und nur zwei Sprachen im Projekt.
- MCP-Server: welche Tools, welche Felder für die KI sichtbar sind, und ob die Datenschutzfreigabe die Übermittlung von Klartext an den KI-Anbieter einschließt.
- Anhänge-Ordner: Liegt im Klartext im Nextcloud-Ordner. Zu klären, ob der Nextcloud-Server vertrauenswürdig ist oder der Ordner zusätzlich verschlüsselt wird.
- Android: Tauri 2 oder PWA; Ordnerzugriff auf Android ist eingeschränkt.
- Tauri-Webview unter Windows (WebView2) und Linux (WebKitGTK) testen.
- Verarbeitungsverzeichnis an die neue Architektur anpassen.
