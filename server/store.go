package main

import (
	"database/sql"
	"errors"
	"fmt"
	"time"

	_ "modernc.org/sqlite"
)

// Entry ist ein verschlüsselter Eintrag. Der Server kennt weder Typ noch Inhalt.
type Entry struct {
	ID        string `json:"id"`
	Seq       int64  `json:"seq"`
	Blob      []byte `json:"blob"`
	UpdatedAt string `json:"updated_at"`
}

// KeyringRecord ist der (für den Server undurchsichtige) Keyring mit Versionsnummer.
type KeyringRecord struct {
	Version   int64  `json:"version"`
	Data      []byte `json:"-"`
	UpdatedAt string `json:"updated_at"`
}

// ErrConflict zeigt an, dass der Stand, auf dem eine Änderung beruht, nicht mehr aktuell ist.
var ErrConflict = errors.New("conflict")

type Store struct {
	db *sql.DB
}

const schema = `
CREATE TABLE IF NOT EXISTS entries (
	id         TEXT PRIMARY KEY,
	seq        INTEGER NOT NULL UNIQUE,
	blob       BLOB NOT NULL,
	updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS keyring (
	id         INTEGER PRIMARY KEY CHECK (id = 1),
	version    INTEGER NOT NULL,
	data       BLOB NOT NULL,
	updated_at TEXT NOT NULL
);
`

// OpenStore öffnet (oder legt an) die SQLite-Datenbank. Schreibzugriffe laufen über eine einzige
// Verbindung, damit die Folgenummern lückenlos und ohne Wettlauf vergeben werden.
func OpenStore(path string) (*Store, error) {
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	for _, pragma := range []string{
		"PRAGMA journal_mode = WAL",
		"PRAGMA synchronous = FULL",
		"PRAGMA busy_timeout = 5000",
	} {
		if _, err := db.Exec(pragma); err != nil {
			db.Close()
			return nil, fmt.Errorf("%s: %w", pragma, err)
		}
	}
	if _, err := db.Exec(schema); err != nil {
		db.Close()
		return nil, fmt.Errorf("Schema anlegen: %w", err)
	}
	return &Store{db: db}, nil
}

func (s *Store) Close() error { return s.db.Close() }

func now() string { return time.Now().UTC().Format(time.RFC3339Nano) }

// Latest liefert die höchste vergebene Folgenummer (0, wenn es noch keine Einträge gibt).
func (s *Store) Latest() (int64, error) {
	var seq int64
	err := s.db.QueryRow(`SELECT COALESCE(MAX(seq), 0) FROM entries`).Scan(&seq)
	return seq, err
}

// Since liefert Einträge mit Folgenummer größer als since, aufsteigend, höchstens limit Stück. more zeigt an,
// dass es noch weitere gibt.
func (s *Store) Since(since int64, limit int) (entries []Entry, more bool, err error) {
	rows, err := s.db.Query(
		`SELECT id, seq, blob, updated_at FROM entries WHERE seq > ? ORDER BY seq ASC LIMIT ?`,
		since, limit+1,
	)
	if err != nil {
		return nil, false, err
	}
	defer rows.Close()
	for rows.Next() {
		var e Entry
		if err := rows.Scan(&e.ID, &e.Seq, &e.Blob, &e.UpdatedAt); err != nil {
			return nil, false, err
		}
		entries = append(entries, e)
	}
	if err := rows.Err(); err != nil {
		return nil, false, err
	}
	if len(entries) > limit {
		entries = entries[:limit]
		more = true
	}
	return entries, more, nil
}

// Get liefert einen Eintrag oder sql.ErrNoRows.
func (s *Store) Get(id string) (Entry, error) {
	var e Entry
	err := s.db.QueryRow(`SELECT id, seq, blob, updated_at FROM entries WHERE id = ?`, id).
		Scan(&e.ID, &e.Seq, &e.Blob, &e.UpdatedAt)
	return e, err
}

// Put schreibt einen Eintrag, wenn baseSeq der aktuellen Folgenummer des Eintrags entspricht (0 für neue
// Einträge). Sonst liefert es ErrConflict und den aktuellen Stand (leer, wenn der Eintrag nicht existiert).
func (s *Store) Put(id string, baseSeq int64, blob []byte) (Entry, error) {
	tx, err := s.db.Begin()
	if err != nil {
		return Entry{}, err
	}
	defer tx.Rollback()

	var current Entry
	err = tx.QueryRow(`SELECT id, seq, blob, updated_at FROM entries WHERE id = ?`, id).
		Scan(&current.ID, &current.Seq, &current.Blob, &current.UpdatedAt)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		current = Entry{}
	case err != nil:
		return Entry{}, err
	}
	if current.Seq != baseSeq {
		return current, ErrConflict
	}

	var maxSeq int64
	if err := tx.QueryRow(`SELECT COALESCE(MAX(seq), 0) FROM entries`).Scan(&maxSeq); err != nil {
		return Entry{}, err
	}
	e := Entry{ID: id, Seq: maxSeq + 1, Blob: blob, UpdatedAt: now()}
	if _, err := tx.Exec(
		`INSERT INTO entries (id, seq, blob, updated_at) VALUES (?, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET seq = excluded.seq, blob = excluded.blob, updated_at = excluded.updated_at`,
		e.ID, e.Seq, e.Blob, e.UpdatedAt,
	); err != nil {
		return Entry{}, err
	}
	if err := tx.Commit(); err != nil {
		return Entry{}, err
	}
	return e, nil
}

// Keyring liefert den gespeicherten Keyring oder sql.ErrNoRows.
func (s *Store) Keyring() (KeyringRecord, error) {
	var k KeyringRecord
	err := s.db.QueryRow(`SELECT version, data, updated_at FROM keyring WHERE id = 1`).
		Scan(&k.Version, &k.Data, &k.UpdatedAt)
	return k, err
}

// PutKeyring speichert den Keyring, wenn baseVersion der aktuellen Version entspricht (0, wenn es noch
// keinen gibt). Sonst ErrConflict.
func (s *Store) PutKeyring(baseVersion int64, data []byte) (KeyringRecord, error) {
	tx, err := s.db.Begin()
	if err != nil {
		return KeyringRecord{}, err
	}
	defer tx.Rollback()

	var current int64
	err = tx.QueryRow(`SELECT version FROM keyring WHERE id = 1`).Scan(&current)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return KeyringRecord{}, err
	}
	if current != baseVersion {
		return KeyringRecord{Version: current}, ErrConflict
	}
	k := KeyringRecord{Version: current + 1, Data: data, UpdatedAt: now()}
	if _, err := tx.Exec(
		`INSERT INTO keyring (id, version, data, updated_at) VALUES (1, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET version = excluded.version, data = excluded.data, updated_at = excluded.updated_at`,
		k.Version, k.Data, k.UpdatedAt,
	); err != nil {
		return KeyringRecord{}, err
	}
	if err := tx.Commit(); err != nil {
		return KeyringRecord{}, err
	}
	return k, nil
}
