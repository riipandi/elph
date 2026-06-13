package renderer

import (
	"testing"

	tea "charm.land/bubbletea/v2"
	uv "github.com/charmbracelet/ultraviolet"
	"github.com/stretchr/testify/require"
)

func decodeKeyPress(t *testing.T, seq []byte) tea.KeyPressMsg {
	t.Helper()
	dec := uv.EventDecoder{}
	n, ev := dec.Decode(seq)
	require.Greater(t, n, 0)
	k, ok := ev.(uv.KeyPressEvent)
	require.Truef(t, ok, "got %T for %q", ev, seq)
	return tea.KeyPressMsg{Code: k.Code, Mod: tea.KeyMod(k.Mod), Text: k.Text}
}

func TestGhosttyOptionDeleteSequences(t *testing.T) {
	cases := []struct {
		name string
		seq  []byte
	}{
		{name: "meta backspace", seq: []byte("\x1b\x7f")},
		{name: "csi alt delete", seq: []byte("\x1b[3;3~")},
		{name: "csi cmd delete", seq: []byte("\x1b[3;9~")},
		{name: "kitty alt backspace", seq: []byte("\x1b[127;2u")},
		{name: "ctrl w", seq: []byte("\x17")},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			key := decodeKeyPress(t, tc.seq)
			t.Logf("seq=%q string=%q keystroke=%q", tc.seq, key.String(), key.Keystroke())
		})
	}
}