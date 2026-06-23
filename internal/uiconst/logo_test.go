package uiconst

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestLogoReturnsTwoLines(t *testing.T) {
	out := Logo()
	lines := strings.SplitN(out, "\n", 3)
	require.Len(t, lines, 2)
	require.NotEmpty(t, lines[0])
	require.NotEmpty(t, lines[1])
}

func TestLogoLinesReturnsTwoElements(t *testing.T) {
	lines := LogoLines()
	require.Len(t, lines, 2)
	require.Equal(t, LogoLine1, lines[0])
	require.Equal(t, LogoLine2, lines[1])
}

func TestJoinSideBySide_Normal(t *testing.T) {
	got := JoinSideBySide([]string{"left"}, []string{"right"}, 2)
	require.Contains(t, got, "left")
	require.Contains(t, got, "right")
}

func TestJoinSideBySide_DifferentHeights(t *testing.T) {
	got := JoinSideBySide([]string{"a", "b"}, []string{"c"}, 1)
	lines := strings.Split(got, "\n")
	require.Len(t, lines, 2)
}

func TestJoinSideBySide_BothEmpty(t *testing.T) {
	got := JoinSideBySide(nil, nil, 2)
	require.Equal(t, "", got)
}

func TestJoinSideBySide_LeftEmpty(t *testing.T) {
	got := JoinSideBySide(nil, []string{"a", "b"}, 1)
	lines := strings.Split(got, "\n")
	require.Len(t, lines, 2)
}

func TestJoinSideBySide_WithDifferentLengths(t *testing.T) {
	got := JoinSideBySide([]string{"short"}, []string{"longer content"}, 2)
	lines := strings.Split(got, "\n")
	require.Len(t, lines, 1)
	require.Contains(t, lines[0], "short")
	require.Contains(t, lines[0], "longer content")
}

func TestJoinSideBySide_GapRepeated(t *testing.T) {
	got := JoinSideBySide([]string{"a"}, []string{"b"}, 4)
	require.True(t, strings.Contains(got, "    b"), "gap should be 4 spaces")
}
