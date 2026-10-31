# Sync-Server

Speichert und verteilt verschlüsselte Einträge. Er kennt weder Typ noch Inhalt der Einträge. Protokoll: `../docs/ARCHITEKTUR.md`.

Konfiguration über Umgebungsvariablen:

| Variable | Bedeutung | Standard |
|---|---|---|
| `BDBB_TOKEN` | Bearer-Token, Pflicht, mindestens 32 Zeichen | |
| `BDBB_ADDR` | Adresse zum Lauschen | `:8080` |
| `BDBB_DB` | Pfad der SQLite-Datei | `./data/bdbb.db` |

Ein zufälliges Token erzeugt man zum Beispiel mit `openssl rand -base64 36`.

```bash
BDBB_TOKEN=... go run .
go test -race ./...
```

Der Server spricht nur HTTP. TLS übernimmt der Caddy davor. Go-Version: siehe `go.mod` (Go lädt die passende Version automatisch nach).
