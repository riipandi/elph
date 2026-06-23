package align

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestTruncateDisplayWidth_ReturnsEmptyWhenMaxWLessThanOne(t *testing.T) {
	require.Equal(t, "", TruncateDisplayWidth("hello", 0))
	require.Equal(t, "", TruncateDisplayWidth("hello", -1))
}

func TestTruncateDisplayWidth_ReturnsInputWhenShorterOrEqual(t *testing.T) {
	require.Equal(t, "hi", TruncateDisplayWidth("hi", 5))
	require.Equal(t, "hello", TruncateDisplayWidth("hello", 5))
}

func TestTruncateDisplayWidth_TruncatesAtBoundary(t *testing.T) {
	require.Equal(t, "he", TruncateDisplayWidth("hello world", 2))
	require.Equal(t, "hello", TruncateDisplayWidth("hello world", 5))
}

func TestTruncateDisplayWidth_HandlesCJKCharacters(t *testing.T) {
	// CJK characters are typically 2 display cells wide
	got := TruncateDisplayWidth("你好世界", 4)
	require.Equal(t, "你好", got)
}

func TestTruncateDisplayWidth_EmptyString(t *testing.T) {
	require.Equal(t, "", TruncateDisplayWidth("", 10))
}

func TestTruncateDisplayWidth_StopsMidRuneAtEdge(t *testing.T) {
	// Two CJK chars (4 cells), maxW=3 should truncate to first CJK char only
	got := TruncateDisplayWidth("你好", 2)
	require.Equal(t, "你", got)
}
