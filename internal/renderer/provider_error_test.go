package renderer

import (
	"errors"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestFinishAgentTurnProviderErrorDetail(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	err := &provider.ProviderError{
		Message:    "unexpected end of JSON input",
		StatusCode: 502,
		URL:        "https://opencode.ai/zen/go/v1/chat/completions",
		ResponseBody: []byte(""),
	}
	m, _ = m.finishAgentTurn("", provider.ProviderErrorSummary(err), err)

	require.Len(t, m.messages, 2)
	require.Equal(t, constants.MessageAI, m.messages[0].kind)
	require.Contains(t, m.messages[0].text, "Provider error:")
	require.Contains(t, m.messages[0].text, "unexpected end of JSON input")

	require.Equal(t, constants.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Provider error", m.messages[1].detailLabel)
	require.Equal(t, constants.DetailStatusError, m.messages[1].detailStatus)
	require.Contains(t, m.messages[1].text, "Provider request failed")
	require.Contains(t, m.messages[1].text, "https://opencode.ai/zen/go/v1/chat/completions")
}

func TestProviderErrorDetailCollapsedShowsFailedPreview(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:         constants.MessageDetail,
		detailLabel:  "Provider error",
		text:         "Provider request failed\n\nunexpected end of JSON input",
		detailStatus: constants.DetailStatusError,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Failed")
	require.NotContains(t, rendered, "unexpected end of JSON input")
}

func TestProviderErrorDetailExpandedShowsBody(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:           constants.MessageDetail,
		detailLabel:    "Provider error",
		text:           "Provider request failed\n\nunexpected end of JSON input",
		detailStatus:   constants.DetailStatusError,
		detailExpanded: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Provider request failed")
	require.Contains(t, rendered, "unexpected end of JSON input")
}

func TestTurnDoneProviderErrorEvent(t *testing.T) {
	err := errors.New("rate limited")
	evt := agent.TurnDoneProviderErrorEvent(err, nil)
	require.Equal(t, agent.EventTurnDone, evt.Kind)
	require.Equal(t, err, evt.ProviderErr)
	require.Equal(t, "Provider error: rate limited", evt.Response)
}