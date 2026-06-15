package renderer

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestAskUserQuestionAndOptions(t *testing.T) {
	fields := parseAskUserArgs(map[string]any{
		"question": "Go or Rust?",
		"options":  []any{"Go", "Rust"},
	})
	require.Equal(t, "Go or Rust?", fields.question)
	require.Equal(t, []string{"Go", "Rust"}, fields.options)
}

func TestParseAskUserArgsOptionsJSONString(t *testing.T) {
	fields := parseAskUserArgs(map[string]any{
		"question": "Pick a language",
		"options":  `["English", "Indonesia"]`,
	})
	require.Equal(t, "Pick a language", fields.question)
	require.Equal(t, []string{"English", "Indonesia"}, fields.options)
}

func TestParseAskUserArgsQuestionHoldsJSONArray(t *testing.T) {
	fields := parseAskUserArgs(map[string]any{
		"question": `["English", "Indonesia"]`,
	})
	require.Equal(t, "Choose an option:", fields.question)
	require.Equal(t, []string{"English", "Indonesia"}, fields.options)
}

func TestParseAskUserArgsSwappedQuestionAndOptions(t *testing.T) {
	fields := parseAskUserArgs(map[string]any{
		"question": `["English", "Indonesia"`,
		"options":  "What language should the report be in",
	})
	require.Equal(t, "What language should the report be in", fields.question)
	require.Equal(t, []string{"English", "Indonesia"}, fields.options)
}

func TestParseAskUserArgsMalformedQuestionSalvagesQuotedOptions(t *testing.T) {
	fields := parseAskUserArgs(map[string]any{
		"question": `["English", "Indonesia`,
	})
	require.Equal(t, "Choose an option:", fields.question)
	require.Equal(t, []string{"English", "Indonesia"}, fields.options)
}

func TestNewAskUserFormShowsQuestionNotRawJSONArray(t *testing.T) {
	form := newAskUserForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractAskUser,
		Args: map[string]any{
			"question": `["English", "Indonesia"`,
			"options":  "What language should the report be in",
		},
	}, 80)
	if updated, _ := form.Update(tea.WindowSizeMsg{Width: 100, Height: 40}); updated != nil {
		if f, ok := updated.(*huh.Form); ok {
			form = f
		}
	}

	m := testInputModel(t)
	m.toolInteractForm = form
	view := stripANSI(m.toolInteractChromeView())
	require.Contains(t, view, "What language should the report be in")
	require.NotContains(t, view, `["English"`)
}

func TestApprovalFormShowsSessionOption(t *testing.T) {
	form := newToolApprovalForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractApproval,
		Name: "Bash",
		Args: map[string]any{"command": "go test ./..."},
	}, 60)
	require.NotNil(t, form)
	if initCmd := form.Init(); initCmd != nil {
		if msg := initCmd(); msg != nil {
			if updated, _ := form.Update(msg); updated != nil {
				if f, ok := updated.(*huh.Form); ok {
					form = f
				}
			}
		}
	}
	if updated, _ := form.Update(tea.WindowSizeMsg{Width: 100, Height: 40}); updated != nil {
		if f, ok := updated.(*huh.Form); ok {
			form = f
		}
	}

	m := testInputModel(t)
	m.toolInteractForm = form
	m.toolInteractPending = toolInteractOffer{
		Req: agent.ToolInteractRequest{Kind: agent.ToolInteractApproval, Name: "Bash"},
	}
	view := stripANSI(m.toolInteractChromeView())
	require.Contains(t, view, "Allow once")
	require.Contains(t, view, "Allow for session")
	require.Contains(t, view, "Deny")
	require.Contains(t, view, "y once")
	require.Contains(t, view, "a session")
}

func TestFormatApprovalDescriptionBash(t *testing.T) {
	desc := formatApprovalDescription("Bash", map[string]any{
		"command":     "go test ./...",
		"description": "Run tests",
	}, 80)
	require.Contains(t, desc, "go test ./...")
	require.Contains(t, desc, "Run tests")
}

func TestFormatApprovalDescriptionTruncatesLongContent(t *testing.T) {
	longCmd := strings.Repeat("curl https://example.com/api/v1/ ", 20)
	desc := formatApprovalDescription("Bash", map[string]any{
		"command":     longCmd,
		"description": strings.Repeat("Run something important. ", 20),
	}, 60)

	lines := strings.Split(desc, "\n")
	require.LessOrEqual(t, len(lines), maxApprovalDescriptionLines)
	require.True(t, strings.HasSuffix(lines[len(lines)-1], "…"))
}

func TestToolInteractApprovalLongContentFitsTerminal(t *testing.T) {
	longCmd := strings.Repeat("curl -X POST https://example.com/api/v1/very/long/path ", 8)
	longDesc := strings.Repeat("Run something important that needs approval. ", 12)

	form := newToolApprovalForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractApproval,
		Name: "Bash",
		Args: map[string]any{"command": longCmd, "description": longDesc},
	}, 70)
	if updated, _ := form.Update(tea.WindowSizeMsg{Width: 80, Height: 24}); updated != nil {
		if f, ok := updated.(*huh.Form); ok {
			form = f
		}
	}

	m := testInputModel(t)
	m.toolInteractForm = form
	m.toolInteractPending = toolInteractOffer{
		Req: agent.ToolInteractRequest{Kind: agent.ToolInteractApproval, Name: "Bash"},
	}
	m = m.syncLayout(false)

	require.LessOrEqual(t, m.renderedViewHeight(), m.height, "full view should fit terminal")
	require.LessOrEqual(t, lipgloss.Height(form.View()), 14, "form should stay compact")
}

func TestToolInteractBridgeDeliverResponse(t *testing.T) {
	bridge := newToolInteractBridge()
	done := make(chan agent.ToolInteractResponse, 1)

	go func() {
		resp, err := bridge.Interact(t.Context(), agent.ToolInteractRequest{
			Kind: agent.ToolInteractAskUser,
			Args: map[string]any{"question": "Hi?"},
		})
		require.NoError(t, err)
		done <- resp
	}()

	msg := waitToolInteractOffer(bridge)().(toolInteractOfferMsg)
	msg.offer.RespCh <- agent.ToolInteractResponse{Answer: "yes"}

	resp := <-done
	require.Equal(t, "yes", resp.Answer)
}

func TestToolInteractApprovalSessionShortcut(t *testing.T) {
	m := testInputModel(t)
	respCh := make(chan agent.ToolInteractResponse, 1)
	m.toolInteractForm = newToolApprovalForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractApproval,
		Name: "Bash",
	}, 60)
	m.toolInteractPending = toolInteractOffer{
		Req:    agent.ToolInteractRequest{Kind: agent.ToolInteractApproval, Name: "Bash"},
		RespCh: respCh,
	}

	resp, ok := m.toolInteractShortcutResponse(tea.KeyPressMsg{Code: 'a', Text: "a"})
	require.True(t, ok)
	require.True(t, resp.Approved)
	require.True(t, resp.AllowSession)

	updated, _ := m.completeToolInteractWith(resp)
	require.True(t, updated.agent.SessionAllowTools)
	require.True(t, (<-respCh).AllowSession)
}

func TestToolInteractApprovalOnceDoesNotPersistSession(t *testing.T) {
	m := testInputModel(t)
	respCh := make(chan agent.ToolInteractResponse, 1)
	m.toolInteractForm = newToolApprovalForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractApproval,
		Name: "Bash",
	}, 60)
	m.toolInteractPending = toolInteractOffer{
		Req:    agent.ToolInteractRequest{Kind: agent.ToolInteractApproval, Name: "Bash"},
		RespCh: respCh,
	}

	m, _ = m.completeToolInteractWith(agent.ToolInteractResponse{Approved: true})
	require.False(t, m.agent.SessionAllowTools)
	require.False(t, (<-respCh).AllowSession)

	opts := m.buildTurnOptions("next", nil, newToolInteractBridge())
	require.False(t, opts.SkipToolApproval)
}

func TestNormalizeApprovalChoice(t *testing.T) {
	require.Equal(t, approvalChoiceOnce, normalizeApprovalChoice("Allow once"))
	require.Equal(t, approvalChoiceSession, normalizeApprovalChoice("Allow for session"))
	require.Equal(t, approvalChoiceDeny, normalizeApprovalChoice("Deny"))
	require.Equal(t, approvalChoiceOnce, normalizeApprovalChoice(""))
}

func TestToolInteractApprovalYShortcut(t *testing.T) {
	m := testInputModel(t)
	respCh := make(chan agent.ToolInteractResponse, 1)
	m.toolInteractForm = newToolApprovalForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractApproval,
		Name: "Bash",
	}, 60)
	m.toolInteractPending = toolInteractOffer{
		Req:    agent.ToolInteractRequest{Kind: agent.ToolInteractApproval, Name: "Bash"},
		RespCh: respCh,
	}

	resp, ok := m.toolInteractShortcutResponse(tea.KeyPressMsg{Code: 'y', Text: "y"})
	require.True(t, ok)
	require.True(t, resp.Approved)

	updated, _ := m.completeToolInteractWith(resp)
	require.Nil(t, updated.toolInteractForm)
	require.True(t, (<-respCh).Approved)
}

func TestToolInteractAskUserNumberShortcut(t *testing.T) {
	m := testInputModel(t)
	respCh := make(chan agent.ToolInteractResponse, 1)
	m.toolInteractForm = newAskUserForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractAskUser,
		Args: map[string]any{
			"question": "Pick",
			"options":  []any{"A", "B"},
		},
	}, 60)
	m.toolInteractPending = toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractAskUser,
			Args: map[string]any{"options": []any{"A", "B"}},
		},
		RespCh: respCh,
	}

	resp, ok := m.toolInteractShortcutResponse(tea.KeyPressMsg{Code: '2', Text: "2"})
	require.True(t, ok)
	require.Equal(t, "B", resp.Answer)

	updated, _ := m.completeToolInteractWith(resp)
	require.Nil(t, updated.toolInteractForm)
	require.Equal(t, "B", (<-respCh).Answer)
}

func TestToolInteractDialogPolishedLayout(t *testing.T) {
	form := newAskUserForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractAskUser,
		Args: map[string]any{
			"question": "Pick one",
			"options":  []any{"A", "B"},
		},
	}, 60)
	if updated, _ := form.Update(tea.WindowSizeMsg{Width: 100, Height: 40}); updated != nil {
		if f, ok := updated.(*huh.Form); ok {
			form = f
		}
	}

	m := testInputModel(t)
	m.toolInteractForm = form
	m.toolInteractPending = toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractAskUser,
			Args: map[string]any{
				"question": "Pick one",
				"options":  []any{"A", "B"},
			},
		},
	}

	view := stripANSI(m.toolInteractChromeView())
	require.Contains(t, view, "Question")
	require.Contains(t, view, "Pick one")
	require.Contains(t, view, "↑/↓ · 1-9")
	require.NotContains(t, view, "↑ up")
}
