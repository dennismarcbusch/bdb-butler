// Sync-Server des BdB-Butlers. Er speichert und verteilt nur Chiffretext.
//
// Konfiguration über Umgebungsvariablen:
//
//	BDBB_TOKEN  Bearer-Token (Pflicht, mindestens 32 Zeichen)
//	BDBB_ADDR   Adresse zum Lauschen (Standard :8080)
//	BDBB_DB     Pfad der SQLite-Datei (Standard ./data/bdbb.db)
package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"
)

const minTokenLength = 32

func main() {
	token := os.Getenv("BDBB_TOKEN")
	if len(token) < minTokenLength {
		log.Fatalf("BDBB_TOKEN fehlt oder ist kürzer als %d Zeichen", minTokenLength)
	}
	addr := getenv("BDBB_ADDR", ":8080")
	dbPath := getenv("BDBB_DB", "./data/bdbb.db")

	if err := os.MkdirAll(filepath.Dir(dbPath), 0o700); err != nil {
		log.Fatalf("Datenordner anlegen: %v", err)
	}
	store, err := OpenStore(dbPath)
	if err != nil {
		log.Fatalf("Datenbank öffnen: %v", err)
	}
	defer store.Close()

	srv := &http.Server{
		Addr:              addr,
		Handler:           NewServer(store, token).Handler(),
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       30 * time.Second,
		WriteTimeout:      30 * time.Second,
		IdleTimeout:       60 * time.Second,
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	go func() {
		<-ctx.Done()
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutdownCtx)
	}()

	log.Printf("lausche auf %s, Datenbank %s", addr, dbPath)
	if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatalf("Server: %v", err)
	}
}

func getenv(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}
