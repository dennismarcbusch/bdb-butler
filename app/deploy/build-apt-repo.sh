#!/bin/sh
# Erzeugt aus .deb-Dateien ein signiertes apt-Repository (reine Dateien, Hosting über GitHub Pages).
#
# Aufruf:
#
#   APT_GPG_PRIVATE_KEY="$(cat schluessel.asc)" sh build-apt-repo.sh <Ordner mit neuen .deb> <Repository-Ordner>
#
# Der Repository-Ordner darf schon ein früheres Repository enthalten. Die neuen Pakete kommen nach pool/ dazu, die
# Indizes werden aus allen Paketen dort neu erzeugt. Es bleiben die neuesten APT_KEEP Versionen (Standard 3), damit
# man bei Bedarf zurückgehen kann und der Ordner nicht wächst.
#
# Ergebnis im Repository-Ordner:
#
#   pool/*.deb                                  die Pakete
#   dists/<Suite>/main/binary-amd64/Packages    Paketliste (auch als .gz)
#   dists/<Suite>/Release, InRelease, Release.gpg   Prüfsummen, mit dem Schlüssel signiert
#   bdb-butler.gpg                              öffentlicher Schlüssel für apt (Binärformat, signed-by)
#   bdb-butler.asc                              derselbe Schlüssel als Text
#   .nojekyll                                   GitHub Pages soll die Dateien unverändert ausliefern
#
# Umgebungsvariablen:
#
#   APT_GPG_PRIVATE_KEY   geheimer Schlüssel als Text (gpg --armor --export-secret-keys), Pflicht
#   APT_GPG_PASSPHRASE    Passwort des Schlüssels, nur wenn er eines hat (Standard: keines)
#   APT_SUITE             Name der Suite (Standard stable)
#   APT_ARCH              Architektur der Pakete (Standard amd64)
#   APT_KEEP              Anzahl der Versionen, die im pool/ bleiben (Standard 3)
#
# Der geheime Schlüssel liegt nur in einem temporären Ordner und wird am Ende gelöscht. Er wird nie ausgegeben.

set -eu

if [ "$#" -ne 2 ]; then
  echo "Aufruf: build-apt-repo.sh <Ordner mit neuen .deb> <Repository-Ordner>" >&2
  exit 2
fi

NEW_DEBS="$1"
REPO="$2"
SUITE="${APT_SUITE:-stable}"
ARCH="${APT_ARCH:-amd64}"
KEEP="${APT_KEEP:-3}"
COMPONENT=main

if [ -z "${APT_GPG_PRIVATE_KEY:-}" ]; then
  echo "APT_GPG_PRIVATE_KEY fehlt. Ohne Signatur nimmt apt das Repository nicht an." >&2
  echo "Schlüssel anlegen und als Secret hinterlegen: siehe docs/INSTALLATION.md, Abschnitt 4.1." >&2
  exit 1
fi
for tool in apt-ftparchive dpkg-deb gpg gzip; do
  command -v "$tool" >/dev/null 2>&1 || { echo "Es fehlt: $tool (Pakete apt-utils, dpkg, gnupg)" >&2; exit 1; }
done

set -- "$NEW_DEBS"/*.deb
if [ ! -e "$1" ]; then
  echo "Keine .deb-Dateien in $NEW_DEBS" >&2
  exit 1
fi

GNUPGHOME="$(mktemp -d)"
export GNUPGHOME
chmod 700 "$GNUPGHOME"
trap 'gpgconf --kill gpg-agent >/dev/null 2>&1 || true; rm -rf "$GNUPGHOME"' EXIT

# Schlüssel einspielen. Die Passphrase kommt über eine Datei, nicht über die Befehlszeile.
printf '%s\n' "$APT_GPG_PRIVATE_KEY" | gpg --batch --quiet --import
KEY_ID="$(gpg --batch --list-secret-keys --with-colons | awk -F: '$1 == "fpr" { print $10; exit }')"
if [ -z "$KEY_ID" ]; then
  echo "Im Schlüssel steckt kein geheimer Schlüssel." >&2
  exit 1
fi
PASS_FILE="$GNUPGHOME/passphrase"
printf '%s' "${APT_GPG_PASSPHRASE:-}" > "$PASS_FILE"
chmod 600 "$PASS_FILE"
sign() {
  gpg --batch --yes --quiet --pinentry-mode loopback --passphrase-file "$PASS_FILE" --local-user "$KEY_ID" "$@"
}

# Pakete übernehmen und alte Versionen aufräumen.
mkdir -p "$REPO/pool"
cp -f "$NEW_DEBS"/*.deb "$REPO/pool/"
# Nach Paketname gruppieren und je Name nur die neuesten Versionen behalten.
for name in $(for f in "$REPO"/pool/*.deb; do dpkg-deb --field "$f" Package; done | sort -u); do
  for f in "$REPO"/pool/*.deb; do
    if [ "$(dpkg-deb --field "$f" Package)" = "$name" ]; then
      printf '%s %s\n' "$(dpkg-deb --field "$f" Version)" "$f"
    fi
  done | sort -t' ' -k1,1 -V | head -n "-$KEEP" | while read -r _ old; do
    echo "Entferne alte Version: $old"
    rm -f "$old"
  done
done

# Indizes erzeugen. apt-ftparchive muss im Repository-Ordner laufen, damit "Filename" relativ ist.
BIN_DIR="dists/$SUITE/$COMPONENT/binary-$ARCH"
(
  cd "$REPO"
  rm -rf "dists/$SUITE"
  mkdir -p "$BIN_DIR"
  apt-ftparchive packages pool > "$BIN_DIR/Packages"
  gzip -9 -n -k -f "$BIN_DIR/Packages"
  apt-ftparchive \
    -o "APT::FTPArchive::Release::Origin=BdB-Butler" \
    -o "APT::FTPArchive::Release::Label=BdB-Butler" \
    -o "APT::FTPArchive::Release::Suite=$SUITE" \
    -o "APT::FTPArchive::Release::Codename=$SUITE" \
    -o "APT::FTPArchive::Release::Architectures=$ARCH" \
    -o "APT::FTPArchive::Release::Components=$COMPONENT" \
    release "dists/$SUITE" > "dists/$SUITE/Release"
)

# Signieren: InRelease (Klartext-Signatur, das nimmt apt zuerst) und Release.gpg (abgetrennt, für ältere Clients).
sign --clearsign --output "$REPO/dists/$SUITE/InRelease" "$REPO/dists/$SUITE/Release"
sign --armor --detach-sign --output "$REPO/dists/$SUITE/Release.gpg" "$REPO/dists/$SUITE/Release"

# Öffentlichen Schlüssel bereitstellen.
gpg --batch --yes --export "$KEY_ID" > "$REPO/bdb-butler.gpg"
gpg --batch --yes --armor --export "$KEY_ID" > "$REPO/bdb-butler.asc"
: > "$REPO/.nojekyll"

echo "Repository fertig: $REPO (Suite $SUITE, Schlüssel $KEY_ID)"
ls -l "$REPO/pool"
