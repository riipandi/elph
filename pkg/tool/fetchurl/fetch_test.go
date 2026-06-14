package fetchurl

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFetchHTML(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		_, _ = w.Write([]byte(`<html><body><script>alert(1)</script><p>Hello <b>world</b></p></body></html>`))
	}))
	defer srv.Close()

	SetAllowPrivateHostsForTest(true)
	t.Cleanup(func() { SetAllowPrivateHostsForTest(false) })
	orig := HTTPClient
	t.Cleanup(func() { HTTPClient = orig })
	HTTPClient = srv.Client()

	result, err := Fetch(context.Background(), srv.URL)
	require.NoError(t, err)
	require.Contains(t, result.Body, "Hello world")
	require.NotContains(t, result.Body, "alert")
}

func TestFetchRejectsLocalhost(t *testing.T) {
	_, err := Fetch(context.Background(), "http://localhost/secret")
	require.Error(t, err)
	require.Contains(t, err.Error(), "localhost")
}

func TestFormatOutput(t *testing.T) {
	out := Format(Result{URL: "https://example.com", ContentType: "text/plain", Body: "hi"})
	require.Contains(t, out, "url: https://example.com")
	require.Contains(t, out, "hi")
}
