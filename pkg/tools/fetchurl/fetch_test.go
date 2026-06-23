package fetchurl

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
	"resty.dev/v3"
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
	HTTPClient = resty.New().SetTransport(srv.Client().Transport)

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

func TestTrimBody_ShortReturnsUnchanged(t *testing.T) {
	require.Equal(t, "short", trimBody("short"))
}

func TestTrimBody_TrimsWhitespace(t *testing.T) {
	require.Equal(t, "hello", trimBody("  hello  "))
}

func TestTrimBody_TruncatesLong(t *testing.T) {
	s := string(make([]byte, 300))
	got := trimBody(s)
	require.Len(t, got, 240+3) // 240 chars + "..."
	require.Contains(t, got, "...")
}

func TestTrimBody_Boundary(t *testing.T) {
	// exactly 240 chars should not be truncated
	s := string(make([]byte, 240))
	got := trimBody(s)
	require.Len(t, got, 240)
	require.NotContains(t, got, "...")
}

func TestFormatOutput_EmptyContentType(t *testing.T) {
	out := Format(Result{URL: "https://example.com", Body: "no type"})
	require.Contains(t, out, "url: https://example.com")
	require.NotContains(t, out, "content_type:")
	require.Contains(t, out, "no type")
}
