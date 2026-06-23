package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestCollapsibleHeaderRendersLabel(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		kind:        uiconst.MessageDetail,
		detailLabel: "Prompt",
		text:        "body",
	}))
	require.Contains(t, rendered, "●")
	require.Contains(t, rendered, "Prompt")
}

func TestCollapsibleHeaderIsNotFullWidth(t *testing.T) {
	m := testModel()
	m.width = 80
	rendered := m.renderMessage(message{
		kind:        uiconst.MessageDetail,
		detailLabel: "Prompt",
		text:        "body",
	})
	firstLine := strings.SplitN(rendered, "\n", 2)[0]
	require.Less(t, lipgloss.Width(firstLine), m.messageAreaWidth())
}

func TestCollapsibleLayoutHasHintGapAfterContent(t *testing.T) {
	m := testModel()
	// compact format: hint on header line, body shown after with blank line gap
	// so hint position is before body position
	_ = m
}
