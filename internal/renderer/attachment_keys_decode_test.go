package renderer

import (
	"testing"

	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/stretchr/testify/require"
)

func decodeSeq(t *testing.T, seq []byte) tea.KeyPressMsg {
	t.Helper()
	dec := uv.EventDecoder{}
	n, ev := dec.Decode(seq)
	require.Greater(t, n, 0)
	k, ok := ev.(uv.KeyPressEvent)
	require.True(t, ok)
	return tea.KeyPressMsg{Code: k.Code, Mod: tea.KeyMod(k.Mod), Text: k.Text}
}

func TestMetaClearDetectsGhosttyCmdDelete(t *testing.T) {
	t.Parallel()

	key := decodeSeq(t, []byte("\x1b[3;9~"))
	require.Equal(t, "meta+delete", key.Keystroke())
	require.True(t, isMetaClearAttachmentsKey(key), "string=%q mod=%v", key.String(), key.Mod)
}

func TestMetaClearCSIPayloadGhosttyCmdDelete(t *testing.T) {
	t.Parallel()

	require.True(t, isMetaClearCSIPayload("3;9~"))
	require.False(t, isMetaClearCSIPayload("3;3~"))
}
