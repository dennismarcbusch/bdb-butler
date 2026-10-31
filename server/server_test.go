package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

const testToken = "0123456789abcdef0123456789abcdef-test"

func newTestServer(t *testing.T) (*httptest.Server, string) {
	t.Helper()
	dbPath := filepath.Join(t.TempDir(), "test.db")
	store, err := OpenStore(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	ts := httptest.NewServer(NewServer(store, testToken).Handler())
	t.Cleanup(func() {
		ts.Close()
		store.Close()
	})
	return ts, dbPath
}

func do(t *testing.T, method, url, token string, body any) (int, map[string]any) {
	t.Helper()
	var reader *bytes.Reader
	switch b := body.(type) {
	case nil:
		reader = bytes.NewReader(nil)
	case string:
		reader = bytes.NewReader([]byte(b))
	default:
		raw, err := json.Marshal(b)
		if err != nil {
			t.Fatal(err)
		}
		reader = bytes.NewReader(raw)
	}
	req, err := http.NewRequest(method, url, reader)
	if err != nil {
		t.Fatal(err)
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var out map[string]any
	_ = json.NewDecoder(resp.Body).Decode(&out)
	return resp.StatusCode, out
}

func putEntry(t *testing.T, ts *httptest.Server, id string, baseSeq int64, blob string) (int, map[string]any) {
	t.Helper()
	return do(t, "PUT", ts.URL+"/v1/entries/"+id, testToken, map[string]any{
		"base_seq": baseSeq,
		"blob":     base64.StdEncoding.EncodeToString([]byte(blob)),
	})
}

func TestHealthzNeedsNoAuth(t *testing.T) {
	ts, _ := newTestServer(t)
	status, body := do(t, "GET", ts.URL+"/healthz", "", nil)
	if status != 200 || body["status"] != "ok" {
		t.Fatalf("healthz: %d %v", status, body)
	}
}

func TestAuthRequired(t *testing.T) {
	ts, _ := newTestServer(t)
	for _, token := range []string{"", "falsch", testToken + "x"} {
		for _, path := range []string{"/v1/keyring", "/v1/entries"} {
			if status, _ := do(t, "GET", ts.URL+path, token, nil); status != 401 {
				t.Errorf("GET %s mit Token %q: Status %d, erwartet 401", path, token, status)
			}
		}
		if status, _ := do(t, "PUT", ts.URL+"/v1/entries/a", token, `{"base_seq":0,"blob":"AQ=="}`); status != 401 {
			t.Errorf("PUT mit Token %q: Status %d, erwartet 401", token, status)
		}
	}
}

func TestKeyringLifecycle(t *testing.T) {
	ts, _ := newTestServer(t)
	url := ts.URL + "/v1/keyring"

	if status, _ := do(t, "GET", url, testToken, nil); status != 404 {
		t.Fatalf("leerer Keyring: Status %d, erwartet 404", status)
	}

	status, body := do(t, "PUT", url, testToken, `{"base_version":0,"keyring":{"version":1,"x":"y"}}`)
	if status != 200 || body["version"] != float64(1) {
		t.Fatalf("anlegen: %d %v", status, body)
	}

	status, body = do(t, "GET", url, testToken, nil)
	if status != 200 || body["version"] != float64(1) {
		t.Fatalf("lesen: %d %v", status, body)
	}
	keyring, _ := body["keyring"].(map[string]any)
	if keyring["x"] != "y" {
		t.Fatalf("Keyring unverändert zurückgeben: %v", body["keyring"])
	}

	// Veralteter Stand: erneut mit base_version 0.
	status, body = do(t, "PUT", url, testToken, `{"base_version":0,"keyring":{"version":1}}`)
	if status != 409 || body["current_version"] != float64(1) {
		t.Fatalf("Konflikt: %d %v", status, body)
	}

	status, body = do(t, "PUT", url, testToken, `{"base_version":1,"keyring":{"version":1,"x":"z"}}`)
	if status != 200 || body["version"] != float64(2) {
		t.Fatalf("aktualisieren: %d %v", status, body)
	}
}

func TestKeyringValidation(t *testing.T) {
	ts, _ := newTestServer(t)
	url := ts.URL + "/v1/keyring"
	for name, body := range map[string]string{
		"fehlt":       `{"base_version":0}`,
		"kein JSON":   `{"base_version":0,"keyring":}`,
		"unbekanntes": `{"base_version":0,"keyring":{},"extra":1}`,
		"zu groß":     fmt.Sprintf(`{"base_version":0,"keyring":{"a":"%s"}}`, strings.Repeat("x", maxKeyringBytes)),
		"Müll":        `nope`,
	} {
		if status, _ := do(t, "PUT", url, testToken, body); status != 400 && status != 413 {
			t.Errorf("%s: Status %d, erwartet 400 oder 413", name, status)
		}
	}
}

func TestEntryCreateListUpdate(t *testing.T) {
	ts, _ := newTestServer(t)

	status, body := putEntry(t, ts, "aaa", 0, "eins")
	if status != 200 || body["seq"] != float64(1) {
		t.Fatalf("neu: %d %v", status, body)
	}
	status, body = putEntry(t, ts, "bbb", 0, "zwei")
	if status != 200 || body["seq"] != float64(2) {
		t.Fatalf("neu: %d %v", status, body)
	}
	status, body = putEntry(t, ts, "aaa", 1, "eins-neu")
	if status != 200 || body["seq"] != float64(3) {
		t.Fatalf("Update: %d %v", status, body)
	}

	status, body = do(t, "GET", ts.URL+"/v1/entries?since=0", testToken, nil)
	if status != 200 {
		t.Fatalf("Liste: %d %v", status, body)
	}
	entries := body["entries"].([]any)
	if len(entries) != 2 || body["latest"] != float64(3) || body["more"] != false {
		t.Fatalf("Liste: %v", body)
	}
	first := entries[0].(map[string]any)
	second := entries[1].(map[string]any)
	if first["id"] != "bbb" || first["seq"] != float64(2) || second["id"] != "aaa" || second["seq"] != float64(3) {
		t.Fatalf("Reihenfolge nach seq: %v", entries)
	}
	if got, _ := base64.StdEncoding.DecodeString(second["blob"].(string)); string(got) != "eins-neu" {
		t.Fatalf("Blob: %q", got)
	}

	status, body = do(t, "GET", ts.URL+"/v1/entries?since=3", testToken, nil)
	if status != 200 || len(body["entries"].([]any)) != 0 || body["latest"] != float64(3) {
		t.Fatalf("nichts Neues: %d %v", status, body)
	}
}

func TestEntryConflicts(t *testing.T) {
	ts, _ := newTestServer(t)
	putEntry(t, ts, "aaa", 0, "eins")

	// Neuer Eintrag, der schon existiert.
	status, body := putEntry(t, ts, "aaa", 0, "doppelt")
	if status != 409 {
		t.Fatalf("doppelt anlegen: %d %v", status, body)
	}
	current := body["current"].(map[string]any)
	if got, _ := base64.StdEncoding.DecodeString(current["blob"].(string)); string(got) != "eins" || current["seq"] != float64(1) {
		t.Fatalf("aktueller Stand fehlt: %v", body)
	}

	// Veralteter Stand.
	putEntry(t, ts, "aaa", 1, "zwei")
	if status, _ := putEntry(t, ts, "aaa", 1, "veraltet"); status != 409 {
		t.Fatalf("veralteter Stand: %d", status)
	}

	// Unbekannter Eintrag mit base_seq > 0.
	status, body = putEntry(t, ts, "ccc", 5, "x")
	if status != 409 || body["current"] != nil {
		t.Fatalf("unbekannter Eintrag: %d %v", status, body)
	}

	// Nach den Konflikten hat sich nichts verändert.
	_, list := do(t, "GET", ts.URL+"/v1/entries?since=0", testToken, nil)
	if len(list["entries"].([]any)) != 1 || list["latest"] != float64(2) {
		t.Fatalf("Konflikte dürfen nichts schreiben: %v", list)
	}
}

func TestEntryValidation(t *testing.T) {
	ts, _ := newTestServer(t)
	cases := map[string]struct {
		path string
		body string
	}{
		"ID mit Sonderzeichen": {"/v1/entries/a%20b", `{"base_seq":0,"blob":"AQ=="}`},
		"ID zu lang":           {"/v1/entries/" + strings.Repeat("a", 65), `{"base_seq":0,"blob":"AQ=="}`},
		"leerer Blob":          {"/v1/entries/a", `{"base_seq":0,"blob":""}`},
		"fehlender Blob":       {"/v1/entries/a", `{"base_seq":0}`},
		"negative base_seq":    {"/v1/entries/a", `{"base_seq":-1,"blob":"AQ=="}`},
		"unbekanntes Feld":     {"/v1/entries/a", `{"base_seq":0,"blob":"AQ==","typ":"schule"}`},
		"kein Base64":          {"/v1/entries/a", `{"base_seq":0,"blob":"***"}`},
		"Anhängsel":            {"/v1/entries/a", `{"base_seq":0,"blob":"AQ=="} {}`},
		"Blob zu groß": {"/v1/entries/a", fmt.Sprintf(`{"base_seq":0,"blob":"%s"}`,
			base64.StdEncoding.EncodeToString(make([]byte, maxBlobBytes+1)))},
	}
	for name, c := range cases {
		status, _ := do(t, "PUT", ts.URL+c.path, testToken, c.body)
		if status != 400 && status != 413 {
			t.Errorf("%s: Status %d, erwartet 400 oder 413", name, status)
		}
	}
	if _, list := do(t, "GET", ts.URL+"/v1/entries", testToken, nil); len(list["entries"].([]any)) != 0 {
		t.Fatal("ungültige Einträge dürfen nicht gespeichert werden")
	}
}

func TestListPaginationAndParams(t *testing.T) {
	ts, _ := newTestServer(t)
	for i := 0; i < 5; i++ {
		putEntry(t, ts, fmt.Sprintf("e%d", i), 0, "x")
	}

	_, body := do(t, "GET", ts.URL+"/v1/entries?since=0&limit=2", testToken, nil)
	if len(body["entries"].([]any)) != 2 || body["more"] != true || body["latest"] != float64(5) {
		t.Fatalf("Seite 1: %v", body)
	}
	_, body = do(t, "GET", ts.URL+"/v1/entries?since=4&limit=2", testToken, nil)
	if len(body["entries"].([]any)) != 1 || body["more"] != false {
		t.Fatalf("letzte Seite: %v", body)
	}

	for _, query := range []string{"since=-1", "since=abc", "limit=0", "limit=abc"} {
		if status, _ := do(t, "GET", ts.URL+"/v1/entries?"+query, testToken, nil); status != 400 {
			t.Errorf("%s: Status %d, erwartet 400", query, status)
		}
	}
	// Ein zu großes limit wird gekappt, nicht abgelehnt.
	if status, _ := do(t, "GET", ts.URL+"/v1/entries?limit=999999", testToken, nil); status != 200 {
		t.Errorf("großes limit: Status %d, erwartet 200", status)
	}
}

func TestConcurrentWritesToSameEntryHaveOneWinner(t *testing.T) {
	ts, _ := newTestServer(t)
	putEntry(t, ts, "aaa", 0, "start")

	const writers = 10
	var wg sync.WaitGroup
	results := make(chan int, writers)
	for i := 0; i < writers; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			status, _ := putEntry(t, ts, "aaa", 1, fmt.Sprintf("von-%d", i))
			results <- status
		}(i)
	}
	wg.Wait()
	close(results)

	var ok, conflict int
	for status := range results {
		switch status {
		case 200:
			ok++
		case 409:
			conflict++
		default:
			t.Errorf("unerwarteter Status %d", status)
		}
	}
	if ok != 1 || conflict != writers-1 {
		t.Fatalf("erwartet 1 Gewinner und %d Konflikte, bekam %d und %d", writers-1, ok, conflict)
	}
}

func TestDataSurvivesRestart(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "restart.db")

	store, err := OpenStore(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := store.Put("aaa", 0, []byte("blob")); err != nil {
		t.Fatal(err)
	}
	if _, err := store.PutKeyring(0, []byte(`{"v":1}`)); err != nil {
		t.Fatal(err)
	}
	store.Close()

	store, err = OpenStore(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	e, err := store.Get("aaa")
	if err != nil || string(e.Blob) != "blob" || e.Seq != 1 {
		t.Fatalf("Eintrag nach Neustart: %v %v", e, err)
	}
	k, err := store.Keyring()
	if err != nil || k.Version != 1 || string(k.Data) != `{"v":1}` {
		t.Fatalf("Keyring nach Neustart: %v %v", k, err)
	}
	// Folgenummern laufen nach dem Neustart weiter.
	next, err := store.Put("bbb", 0, []byte("zwei"))
	if err != nil || next.Seq != 2 {
		t.Fatalf("Folgenummer nach Neustart: %v %v", next, err)
	}
}
