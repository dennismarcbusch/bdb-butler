#!/bin/sh
# Installiert und startet den Sync-Server des BdB-Butlers auf Alpine Linux (zum Beispiel in einem LXC).
#
# Die Server-Datei kommt fertig gebaut aus dem neuesten Release auf GitHub (Prüfsumme wird kontrolliert). Gibt es
# noch keinen Release oder keine Datei für diese Architektur, baut das Skript den Server aus dem Quelltext.
#
# Aufruf als root:
#
#   wget -O install-alpine.sh https://raw.githubusercontent.com/dennismarcbusch/bdb-butler/main/server/deploy/install-alpine.sh
#   sh install-alpine.sh https://butler.example.de
#
# Die Adresse ist die, unter der dein Reverse-Proxy den Server erreichbar macht. Sie ist optional und dient nur
# der Prüfung und der Ausgabe am Ende.
#
# Erneut ausführen aktualisiert den Server auf den neuesten Stand. Token, Einstellungen und Daten bleiben erhalten.
#
# Einstellungen über Umgebungsvariablen (alle optional):
#
#   BDBB_PORT        Port, auf dem der Server lauscht (Standard 8080). Gilt nur bei der Ersteinrichtung,
#                    danach steht er in /etc/bdbb/bdbb.env.
#   BDBB_PUBLIC_URL  wie das erste Argument: Adresse hinter dem Reverse-Proxy (ohne Standard)
#   BDBB_SOURCE      Woher der Server kommt: auto (Release, sonst Quelltext), release (nur Release) oder
#                    source (immer aus dem Quelltext bauen). Standard auto
#   BDBB_VERSION     Release, zum Beispiel v0.1.0 (Standard latest, der neueste veröffentlichte)
#   BDBB_GITHUB      Repository auf GitHub (Standard dennismarcbusch/bdb-butler)
#   BDBB_REF         Branch oder Tag beim Bauen aus dem Quelltext (Standard main)
#   BDBB_BACKUP      1 = tägliche Sicherung der Datenbank einrichten (Standard 1)
#   BDBB_KEEP_BUILD  1 = Go-Caches nach dem Bauen behalten (schnellere Updates, etwa 1 GB Platz). Standard 0
#
# Ergebnis:
#
#   /usr/local/bin/bdbb-server        Server
#   /etc/bdbb/bdbb.env                Token, Adresse, Datenbankpfad (nur root und Gruppe bdbb lesbar)
#   /etc/init.d/bdbb                  OpenRC-Dienst (startet beim Booten)
#   /var/lib/bdbb/bdbb.db             Datenbank (nur Chiffretext)
#   /var/log/bdbb/bdbb.log            Protokoll (ohne Inhalte und ohne Token)
#   /etc/periodic/daily/bdbb-backup   tägliche Sicherung nach /var/backups/bdbb (14 Tage)

set -eu

PORT="${BDBB_PORT:-8080}"
PUBLIC_URL="${1:-${BDBB_PUBLIC_URL:-}}"
PUBLIC_URL="${PUBLIC_URL%/}"
SOURCE="${BDBB_SOURCE:-auto}"
VERSION="${BDBB_VERSION:-latest}"
GITHUB="${BDBB_GITHUB:-dennismarcbusch/bdb-butler}"
REPO="https://github.com/$GITHUB.git"
REF="${BDBB_REF:-main}"
BACKUP="${BDBB_BACKUP:-1}"
KEEP_BUILD="${BDBB_KEEP_BUILD:-0}"

BASE=/opt/bdb-butler
SRC="$BASE/src"
BIN=/usr/local/bin/bdbb-server
RUNNER=/usr/local/libexec/bdbb/run
CONF_DIR=/etc/bdbb
ENV_FILE="$CONF_DIR/bdbb.env"
DATA_DIR=/var/lib/bdbb
LOG_DIR=/var/log/bdbb
INIT=/etc/init.d/bdbb
BACKUP_JOB=/etc/periodic/daily/bdbb-backup

info() { printf '\n==> %s\n' "$*"; }
warn() { printf '\n!!! %s\n' "$*" >&2; }
die() { printf '\nFehler: %s\n' "$*" >&2; exit 1; }

# ----- Voraussetzungen ---------------------------------------------------------------------------------------

[ "$(id -u)" -eq 0 ] || die "Bitte als root ausführen."
[ -f /etc/alpine-release ] || die "Dieses Skript ist für Alpine Linux gedacht (/etc/alpine-release fehlt)."
command -v rc-service >/dev/null 2>&1 || die "OpenRC fehlt (rc-service nicht gefunden)."
case "$PORT" in
	''|*[!0-9]*) die "BDBB_PORT muss eine Zahl sein." ;;
esac
case "$PUBLIC_URL" in
	''|https://*|http://*) ;;
	*) die "Die Adresse muss mit https:// beginnen, zum Beispiel https://butler.example.de" ;;
esac

case "$SOURCE" in
	auto|release|source) ;;
	*) die "BDBB_SOURCE muss auto, release oder source sein." ;;
esac

case "$(uname -m)" in
	x86_64) ARCH=amd64 ;;
	aarch64|arm64) ARCH=arm64 ;;
	*) ARCH="" ;;
esac

info "Pakete installieren"
apk add --no-cache ca-certificates curl sqlite >/dev/null

# ----- Nutzer und Ordner -------------------------------------------------------------------------------------

info "Systemnutzer und Ordner anlegen"
grep -q '^bdbb:' /etc/group || addgroup -S bdbb
id bdbb >/dev/null 2>&1 || adduser -S -D -H -h "$DATA_DIR" -s /sbin/nologin -G bdbb -g "BdB-Butler Sync-Server" bdbb
install -d -o bdbb -g bdbb -m 0700 "$DATA_DIR"
install -d -o bdbb -g bdbb -m 0750 "$LOG_DIR"
install -d -o root -g bdbb -m 0750 "$CONF_DIR"
install -d -m 0755 "$BASE" "$(dirname "$RUNNER")"

# Ersetzt die Server-Datei. Umbenennen statt Überschreiben, damit ein laufender Server nicht stört.
install_binary() {
	chmod 0755 "$1"
	mv -f "$1" "$BIN"
}

# ----- Fertige Datei aus dem Release -------------------------------------------------------------------------

# Lädt die Server-Datei aus dem Release und prüft sie gegen SHA256SUMS-server.txt. Gelingt das nicht, Rückgabe 1.
from_release() {
	[ -n "$ARCH" ] || { echo "Keine fertige Datei für $(uname -m)."; return 1; }
	if [ "$VERSION" = "latest" ]; then
		base="https://github.com/$GITHUB/releases/latest/download"
		tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$GITHUB/releases/latest" 2>/dev/null | sed -n 's|.*/tag/||p')
	else
		base="https://github.com/$GITHUB/releases/download/$VERSION"
		tag="$VERSION"
	fi
	[ -n "$tag" ] || { echo "Es gibt noch keinen veröffentlichten Release."; return 1; }
	file="bdbb-server-linux-$ARCH"
	tmp=$(mktemp -d)
	if ! curl -fsL --retry 3 -o "$tmp/$file" "$base/$file" || ! curl -fsL --retry 3 -o "$tmp/SHA256SUMS" "$base/SHA256SUMS-server.txt"; then
		rm -rf "$tmp"
		echo "Release $tag enthält keine Server-Datei für $ARCH."
		return 1
	fi
	if ! (cd "$tmp" && grep " $file\$" SHA256SUMS | sha256sum -c - >/dev/null 2>&1); then
		rm -rf "$tmp"
		die "Die Prüfsumme der heruntergeladenen Datei stimmt nicht. Abgebrochen, nichts wurde ersetzt."
	fi
	install_binary "$tmp/$file"
	rm -rf "$tmp"
	COMMIT="Release $tag"
	echo "Server aus Release $tag geladen und geprüft."
}

# ----- Bauen aus dem Quelltext -------------------------------------------------------------------------------

from_source() {
	mem_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
	if [ "${mem_kb:-0}" -lt 1500000 ]; then
		warn "Der Container hat weniger als 1,5 GB RAM. Das Bauen (SQLite-Treiber in Go) braucht kurzzeitig viel \
Speicher und kann abbrechen. Dann vorübergehend mehr RAM oder Swap zuweisen."
	fi
	apk add --no-cache git go >/dev/null

	info "Quelltext holen ($REPO, $REF)"
	if [ -d "$SRC/.git" ]; then
		git -C "$SRC" fetch --quiet --depth 1 origin "$REF"
		git -C "$SRC" reset --quiet --hard FETCH_HEAD
	else
		rm -rf "$SRC"
		git clone --quiet --depth 1 --branch "$REF" "$REPO" "$SRC"
	fi
	COMMIT="Quelltext $(git -C "$SRC" rev-parse --short HEAD)"

	info "Server bauen (das dauert beim ersten Mal einige Minuten)"
	# Alpines Go-Paket ist oft älter als die Version in go.mod. GOTOOLCHAIN=auto lädt dann die passende Version
	# von proxy.golang.org (die offiziellen Go-Werkzeuge sind statisch gelinkt und laufen auch mit musl).
	(
		export GOTOOLCHAIN=auto GOPATH="$BASE/go" GOCACHE="$BASE/go-cache" GOFLAGS=-mod=readonly CGO_ENABLED=0
		cd "$SRC/server"
		go build -trimpath -ldflags "-s -w" -o "$BIN.new" .
		if [ "$KEEP_BUILD" != "1" ]; then
			go clean -cache -modcache >/dev/null 2>&1 || true
			rm -rf "$GOPATH" "$GOCACHE"
		fi
	)
	install_binary "$BIN.new"
}

COMMIT=""
case "$SOURCE" in
	source)
		from_source
		;;
	release)
		info "Server aus dem Release laden ($GITHUB, $VERSION)"
		from_release || die "Kein passender Release gefunden (BDBB_SOURCE=release)."
		;;
	auto)
		info "Server aus dem Release laden ($GITHUB, $VERSION)"
		if ! from_release; then
			echo "Baue stattdessen aus dem Quelltext."
			from_source
		fi
		;;
esac

# ----- Einstellungen -----------------------------------------------------------------------------------------

NEW_TOKEN=""
if [ -f "$ENV_FILE" ]; then
	info "Einstellungen vorhanden, bleiben unverändert ($ENV_FILE)"
else
	info "Einstellungen und Token anlegen"
	NEW_TOKEN=$(head -c 48 /dev/urandom | base64 | tr -d '\n=+/' | cut -c1-48)
	[ "${#NEW_TOKEN}" -ge 32 ] || die "Token konnte nicht erzeugt werden."
	umask 027
	cat >"$ENV_FILE" <<EOF
# Einstellungen des BdB-Butler-Sync-Servers. Nach Änderungen: rc-service bdbb restart
BDBB_TOKEN=$NEW_TOKEN
BDBB_ADDR=:$PORT
BDBB_DB=$DATA_DIR/bdbb.db
EOF
	umask 022
fi
chown root:bdbb "$ENV_FILE"
chmod 0640 "$ENV_FILE"
LISTEN_PORT=$(sed -n 's/^BDBB_ADDR=.*://p' "$ENV_FILE")

# Startet den Server mit den Einstellungen aus der Datei. So steht das Token nicht in der Prozessliste.
cat >"$RUNNER" <<EOF
#!/bin/sh
set -a
. $ENV_FILE
set +a
exec $BIN
EOF
chmod 0755 "$RUNNER"

# ----- Dienst ------------------------------------------------------------------------------------------------

info "OpenRC-Dienst einrichten"
cat >"$INIT" <<EOF
#!/sbin/openrc-run
# Erzeugt von server/deploy/install-alpine.sh (BdB-Butler).

name="bdbb"
description="BdB-Butler Sync-Server (speichert nur Chiffretext)"

supervisor="supervise-daemon"
command="$RUNNER"
command_user="bdbb:bdbb"
directory="$DATA_DIR"
output_log="$LOG_DIR/bdbb.log"
error_log="$LOG_DIR/bdbb.log"
respawn_delay=5
respawn_max=10
respawn_period=120

depend() {
	need net
	after firewall
}

start_pre() {
	checkpath --directory --owner bdbb:bdbb --mode 0700 "$DATA_DIR"
	checkpath --directory --owner bdbb:bdbb --mode 0750 "$LOG_DIR"
	checkpath --file --owner bdbb:bdbb --mode 0640 "$LOG_DIR/bdbb.log"
}
EOF
chmod 0755 "$INIT"
rc-update add bdbb default >/dev/null 2>&1 || true

if rc-service bdbb status >/dev/null 2>&1; then
	rc-service bdbb restart
else
	rc-service bdbb start
fi

# ----- Sicherung ---------------------------------------------------------------------------------------------

if [ "$BACKUP" = "1" ]; then
	info "Tägliche Sicherung einrichten ($BACKUP_JOB)"
	mkdir -p "$(dirname "$BACKUP_JOB")"
	cat >"$BACKUP_JOB" <<'EOF'
#!/bin/sh
# Tägliche Sicherung der Datenbank des BdB-Butler-Sync-Servers (nur Chiffretext). Behält 14 Tage.
set -eu
. /etc/bdbb/bdbb.env
DIR=/var/backups/bdbb
mkdir -p "$DIR"
chmod 0700 "$DIR"
[ -f "$BDBB_DB" ] || exit 0
sqlite3 "$BDBB_DB" ".backup '$DIR/bdbb-$(date +%F).db'"
find "$DIR" -name 'bdbb-*.db' -mtime +14 -exec rm -f {} \;
EOF
	chmod 0755 "$BACKUP_JOB"
	rc-update add crond default >/dev/null 2>&1 || true
	rc-service crond status >/dev/null 2>&1 || rc-service crond start >/dev/null 2>&1 || true
fi

# ----- Prüfen ------------------------------------------------------------------------------------------------

info "Prüfen"
ok=0
for _ in 1 2 3 4 5 6 7 8 9 10; do
	if curl -fsS --max-time 2 "http://127.0.0.1:$LISTEN_PORT/healthz" >/dev/null 2>&1; then
		ok=1
		break
	fi
	sleep 1
done
if [ "$ok" = "1" ]; then
	echo "Lokal: Server antwortet."
else
	warn "Der Server antwortet lokal nicht. Protokoll: tail -n 50 $LOG_DIR/bdbb.log"
	exit 1
fi

LXC_IP=$(ip -4 -o addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -n 1)
if [ -z "$PUBLIC_URL" ]; then
	echo "Keine öffentliche Adresse angegeben, Prüfung über den Reverse-Proxy übersprungen."
elif curl -fsS --max-time 10 "$PUBLIC_URL/healthz" >/dev/null 2>&1; then
	echo "Über den Reverse-Proxy: $PUBLIC_URL antwortet."
else
	warn "$PUBLIC_URL/healthz ist von hier aus nicht erreichbar. Prüfe, ob der Reverse-Proxy auf \
${LXC_IP:-<IP dieses Containers>}:$LISTEN_PORT zeigt. (Manche Container \
erreichen ihre eigene öffentliche Adresse nicht, dann von einem anderen Rechner aus prüfen.)"
fi

# ----- Ergebnis ----------------------------------------------------------------------------------------------

cat <<EOF

========================================================================================================
 BdB-Butler Sync-Server läuft (Stand $COMMIT)

 Server-Adresse für die App:  ${PUBLIC_URL:-die Adresse deines Reverse-Proxys (https://…)}
 Lauscht auf:                 ${LXC_IP:-<IP dieses Containers>}:$LISTEN_PORT (Ziel des Reverse-Proxys)
EOF
if [ -n "$NEW_TOKEN" ]; then
	cat <<EOF
 Token für die App:           $NEW_TOKEN

 Das Token jetzt in den Passwortmanager übernehmen. Es steht außerdem in $ENV_FILE.
EOF
else
	cat <<EOF
 Token:                       unverändert, siehe $ENV_FILE
EOF
fi
cat <<EOF

 Dienst:      rc-service bdbb status | restart | stop
 Protokoll:   tail -f $LOG_DIR/bdbb.log
 Daten:       $DATA_DIR/bdbb.db
 Update:      dieses Skript erneut ausführen
========================================================================================================
EOF
