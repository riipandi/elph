package agent

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestStripExtractedPayloadsExactQuery(t *testing.T) {
	query := "rekomendasi tempat ngopi kerja dikota Sukabumi 2024"
	calls := []ParsedToolCall{{
		Name:       "WebSearch",
		Parameters: map[string]string{"query": query},
	}}

	clean := StripExtractedPayloads(query, calls)
	require.Empty(t, clean)
}

func TestStripExtractedPayloadsQueryPrefix(t *testing.T) {
	query := "rekomendasi tempat ngopi kerja dikota Sukabumi 2024"
	calls := []ParsedToolCall{{
		Name:       "WebSearch",
		Parameters: map[string]string{"query": query},
	}}

	clean := StripExtractedPayloads("rekomendasi tempat ngopi kerja dikota Sukabumi", calls)
	require.Empty(t, clean)
}

func TestStripExtractedPayloadsKeepsProse(t *testing.T) {
	calls := []ParsedToolCall{{
		Name:       "WebSearch",
		Parameters: map[string]string{"query": "cafe Sukabumi"},
	}}

	raw := "Berikut beberapa rekomendasi kafe di Sukabumi."
	clean := StripExtractedPayloads(raw, calls)
	require.Equal(t, raw, clean)
}
