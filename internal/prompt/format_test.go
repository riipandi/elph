package prompt

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNormalizePromptCollapsesBlankLines(t *testing.T) {
	require.Equal(t, "a\n\nb\nc", normalizePrompt("a\n\n\nb\nc\n"))
}

func TestNormalizePromptTrimsLineTrailingSpace(t *testing.T) {
	require.Equal(t, "line", normalizePrompt("line   \n"))
}

func TestNormalizePromptBlankLineBeforeHeading(t *testing.T) {
	require.Equal(t, "intro\n\n## Output\n- bullet", normalizePrompt("intro\n## Output\n- bullet"))
	require.Equal(t, "## A\n\n### B\nitem", normalizePrompt("## A\n### B\nitem"))
}
