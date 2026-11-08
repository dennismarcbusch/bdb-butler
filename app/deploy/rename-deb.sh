#!/bin/sh
# Gibt dem von Tauri gebauten deb-Paket den Paketnamen "bdb-butler".
#
# Tauri leitet den Paketnamen aus dem Produktnamen "BdB-Butler" ab und trennt ihn an jedem Wechsel von Groß- zu
# Kleinschreibung. Heraus kommt "bd-b-butler". Dieses Skript packt das Paket neu und setzt den Namen (und den
# Dateinamen) auf "bdb-butler", damit "apt install bdb-butler" funktioniert. Den Namen "bd-b-butler" früherer
# Pakete löst die Paketkonfiguration ab (replaces und conflicts in app/src-tauri/tauri.conf.json).
#
# Aufruf: sh rename-deb.sh <Paket.deb> [weitere Pakete]
# Das Ergebnis liegt neben dem Original als bdb-butler_<Version>_<Architektur>.deb, das Original wird gelöscht.
#
# Umgebungsvariable DEB_NAME: anderer Paketname (Standard bdb-butler).

set -eu

NAME="${DEB_NAME:-bdb-butler}"

if [ "$#" -lt 1 ]; then
  echo "Aufruf: rename-deb.sh <Paket.deb> [weitere Pakete]" >&2
  exit 2
fi

for deb in "$@"; do
  work="$(mktemp -d)"
  dpkg-deb -R "$deb" "$work/pkg"
  control="$work/pkg/DEBIAN/control"
  version="$(sed -n 's/^Version: //p' "$control")"
  arch="$(sed -n 's/^Architecture: //p' "$control")"
  if [ -z "$version" ] || [ -z "$arch" ]; then
    echo "Version oder Architektur fehlt in $deb" >&2
    rm -rf "$work"
    exit 1
  fi
  sed -i "s/^Package: .*/Package: $NAME/" "$control"
  out="$(dirname "$deb")/${NAME}_${version}_${arch}.deb"
  # xz statt des Standards der neueren dpkg-Versionen (zstd): Ältere Systeme können zstd nicht entpacken.
  dpkg-deb --root-owner-group -Zxz -b "$work/pkg" "$work/neu.deb" >/dev/null
  rm -rf "$work/pkg"
  [ "$out" = "$deb" ] || rm -f "$deb"
  mv -f "$work/neu.deb" "$out"
  rm -rf "$work"
  echo "$out"
done
