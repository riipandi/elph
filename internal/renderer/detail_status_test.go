package renderer

import (
	"errors"
	"testing"

	"github.com/riipandi/elph/internal/runtime/shell"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestShellDetailStatusTransitions(t *testing.T) {
	require.Equal(t, uiconst.DetailStatusRunning, shellDetailStatus(nil, true))
	require.Equal(t, uiconst.DetailStatusSuccess, shellDetailStatus(&shell.ShellResult{ExitCode: 0}, false))
	require.Equal(t, uiconst.DetailStatusError, shellDetailStatus(&shell.ShellResult{ExitCode: 2}, false))
	require.Equal(t, uiconst.DetailStatusWarning, shellDetailStatus(&shell.ShellResult{Cancelled: true}, false))
	require.Equal(t, uiconst.DetailStatusError, shellDetailStatus(&shell.ShellResult{Err: errors.New("boom")}, false))
}

func TestShellDetailMessageUsesDynamicStatusColors(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:         uiconst.MessageDetail,
		detailLabel:  "$ ls",
		text:         "file.txt",
		detailStatus: uiconst.DetailStatusSuccess,
	}}
	success := m.renderMessageAt(0)
	m.messages[0].detailStatus = uiconst.DetailStatusError
	m.messages[0].renderCache = messageRenderCache{}
	errorState := m.renderMessageAt(0)
	require.NotEqual(t, success, errorState)
}
