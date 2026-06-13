package renderer

import (
	"errors"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestFormatModelsSyncResult(t *testing.T) {
	require.Equal(t, "Model metadata updated: openai.json, anthropic.json", formatModelsSyncResult(modelsSyncDoneMsg{
		result: provider.UpdateModelsResult{Updated: []string{"openai.json", "anthropic.json"}},
	}))
	require.Equal(t, "Model metadata is up to date.", formatModelsSyncResult(modelsSyncDoneMsg{
		result: provider.UpdateModelsResult{},
	}))
	require.Contains(t, formatModelsSyncResult(modelsSyncDoneMsg{
		err: errors.New("network down"),
	}), "Model metadata update failed:")
}

func TestStartModelsSyncShowsStatusMessage(t *testing.T) {
	m := New()
	m.ready = true
	m.width = 100
	m.height = 40
	m.content.SetWidth(100)
	m.content.SetHeight(20)

	updated, cmd := m.startModelsSync()
	require.NotNil(t, cmd)
	require.True(t, updated.modelsSyncing)
	require.Len(t, updated.messages, 1)
	require.Contains(t, stripANSI(updated.messages[0].text), modelsSyncUpdatingLabel)
}
