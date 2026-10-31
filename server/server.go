package main

import (
	"crypto/sha256"
	"crypto/subtle"
	"database/sql"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net/http"
	"regexp"
	"strconv"
	"strings"
)

const (
	maxBlobBytes    = 1 << 20 // 1 MiB je Eintrag
	maxRequestBytes = 2 << 20 // JSON mit Base64 braucht etwas mehr Platz als der Blob
	maxKeyringBytes = 64 << 10
	defaultLimit    = 500
	maxLimit        = 1000
)

var idPattern = regexp.MustCompile(`^[A-Za-z0-9-]{1,64}$`)

// Server stellt das Sync-Protokoll bereit (siehe docs/ARCHITEKTUR.md).
type Server struct {
	store     *Store
	tokenHash [sha256.Size]byte
}

func NewServer(store *Store, token string) *Server {
	return &Server{store: store, tokenHash: sha256.Sum256([]byte(token))}
}

func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("GET /v1/keyring", s.auth(s.getKeyring))
	mux.HandleFunc("PUT /v1/keyring", s.auth(s.putKeyring))
	mux.HandleFunc("GET /v1/entries", s.auth(s.listEntries))
	mux.HandleFunc("PUT /v1/entries/{id}", s.auth(s.putEntry))
	return mux
}

// auth verlangt `Authorization: Bearer <Token>`. Der Vergleich läuft in konstanter Zeit.
func (s *Server) auth(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		header := r.Header.Get("Authorization")
		token, ok := strings.CutPrefix(header, "Bearer ")
		if !ok {
			w.Header().Set("WWW-Authenticate", "Bearer")
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		got := sha256.Sum256([]byte(token))
		if subtle.ConstantTimeCompare(got[:], s.tokenHash[:]) != 1 {
			w.Header().Set("WWW-Authenticate", "Bearer")
			writeError(w, http.StatusUnauthorized, "unauthorized")
			return
		}
		next(w, r)
	}
}

func (s *Server) getKeyring(w http.ResponseWriter, r *http.Request) {
	k, err := s.store.Keyring()
	if errors.Is(err, sql.ErrNoRows) {
		writeError(w, http.StatusNotFound, "no keyring")
		return
	}
	if err != nil {
		serverError(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"version":    k.Version,
		"keyring":    json.RawMessage(k.Data),
		"updated_at": k.UpdatedAt,
	})
}

func (s *Server) putKeyring(w http.ResponseWriter, r *http.Request) {
	var req struct {
		BaseVersion int64           `json:"base_version"`
		Keyring     json.RawMessage `json:"keyring"`
	}
	if !decodeBody(w, r, &req) {
		return
	}
	if len(req.Keyring) == 0 || len(req.Keyring) > maxKeyringBytes || !json.Valid(req.Keyring) {
		writeError(w, http.StatusBadRequest, "invalid keyring")
		return
	}
	k, err := s.store.PutKeyring(req.BaseVersion, req.Keyring)
	if errors.Is(err, ErrConflict) {
		writeJSON(w, http.StatusConflict, map[string]any{"error": "conflict", "current_version": k.Version})
		return
	}
	if err != nil {
		serverError(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"version": k.Version, "updated_at": k.UpdatedAt})
}

func (s *Server) listEntries(w http.ResponseWriter, r *http.Request) {
	since, err := parseInt(r.URL.Query().Get("since"), 0)
	if err != nil || since < 0 {
		writeError(w, http.StatusBadRequest, "invalid since")
		return
	}
	limit, err := parseInt(r.URL.Query().Get("limit"), defaultLimit)
	if err != nil || limit < 1 {
		writeError(w, http.StatusBadRequest, "invalid limit")
		return
	}
	if limit > maxLimit {
		limit = maxLimit
	}
	entries, more, err := s.store.Since(since, int(limit))
	if err != nil {
		serverError(w, err)
		return
	}
	latest, err := s.store.Latest()
	if err != nil {
		serverError(w, err)
		return
	}
	if entries == nil {
		entries = []Entry{}
	}
	writeJSON(w, http.StatusOK, map[string]any{"entries": entries, "latest": latest, "more": more})
}

func (s *Server) putEntry(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	if !idPattern.MatchString(id) {
		writeError(w, http.StatusBadRequest, "invalid id")
		return
	}
	var req struct {
		BaseSeq int64  `json:"base_seq"`
		Blob    []byte `json:"blob"`
	}
	if !decodeBody(w, r, &req) {
		return
	}
	if req.BaseSeq < 0 || len(req.Blob) == 0 || len(req.Blob) > maxBlobBytes {
		writeError(w, http.StatusBadRequest, "invalid entry")
		return
	}
	e, err := s.store.Put(id, req.BaseSeq, req.Blob)
	if errors.Is(err, ErrConflict) {
		var current *Entry
		if e.ID != "" {
			current = &e
		}
		writeJSON(w, http.StatusConflict, map[string]any{"error": "conflict", "current": current})
		return
	}
	if err != nil {
		serverError(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"id": e.ID, "seq": e.Seq, "updated_at": e.UpdatedAt})
}

func decodeBody(w http.ResponseWriter, r *http.Request, v any) bool {
	r.Body = http.MaxBytesReader(w, r.Body, maxRequestBytes)
	dec := json.NewDecoder(r.Body)
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			writeError(w, http.StatusRequestEntityTooLarge, "body too large")
			return false
		}
		writeError(w, http.StatusBadRequest, "invalid json")
		return false
	}
	if _, err := dec.Token(); !errors.Is(err, io.EOF) {
		writeError(w, http.StatusBadRequest, "invalid json")
		return false
	}
	return true
}

func parseInt(value string, def int64) (int64, error) {
	if value == "" {
		return def, nil
	}
	return strconv.ParseInt(value, 10, 64)
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(v); err != nil {
		log.Printf("Antwort schreiben: %v", err)
	}
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

// serverError protokolliert den Fehler (ohne Inhalte) und antwortet neutral.
func serverError(w http.ResponseWriter, err error) {
	log.Printf("interner Fehler: %v", err)
	writeError(w, http.StatusInternalServerError, "internal error")
}
