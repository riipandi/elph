package websearch

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestURLQueryEscape(t *testing.T) {
	got := urlQueryEscape("hello world")
	require.Equal(t, "hello+world", got)

	got = urlQueryEscape("a&b=c")
	require.Equal(t, "a%26b%3Dc", got)
}

func TestTrimHTTPErrorBody_Short(t *testing.T) {
	got := trimHTTPErrorBody([]byte("not found"))
	require.Equal(t, "not found", got)
}

func TestTrimHTTPErrorBody_Empty(t *testing.T) {
	got := trimHTTPErrorBody([]byte(""))
	require.Equal(t, "", got)
}

func TestTrimHTTPErrorBody_TruncatesLong(t *testing.T) {
	s := string(make([]byte, 300))
	got := trimHTTPErrorBody([]byte(s))
	require.Len(t, got, 240+3) // 240 chars + "..."
	require.Contains(t, got, "...")
}

func TestTrimHTTPErrorBody_Boundary(t *testing.T) {
	s := string(make([]byte, 240))
	got := trimHTTPErrorBody([]byte(s))
	require.Len(t, got, 240)
	require.NotContains(t, got, "...")
}

func TestTrimHTTPErrorBody_StripsWhitespace(t *testing.T) {
	got := trimHTTPErrorBody([]byte("  hello  "))
	require.Equal(t, "hello", got)
}
