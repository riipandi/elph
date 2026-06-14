package openai

import (
	"github.com/stretchr/testify/require"
	"testing"
)

func TestReasoningTextOpenCodeMiMoDelta(t *testing.T) {
	raw := `{"content":"","role":"assistant","reasoning":"Okay","reasoning_details":[{"type":"reasoning.text","text":"Okay","format":"unknown","index":0}]}`
	require.Equal(t, "Okay", reasoningText(nil, raw))
}

func TestReasoningTextOpenCodeReasoningDetailsOnly(t *testing.T) {
	raw := `{"content":"","reasoning_details":[{"type":"reasoning.text","text":" step","format":"unknown","index":0}]}`
	require.Equal(t, " step", reasoningText(nil, raw))
}
