package renderer

import (
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestAskUserQuestionAndOptions(t *testing.T) {
	q := askUserQuestion(map[string]any{"question": "Go or Rust?"})
	require.Equal(t, "Go or Rust?", q)

	opts := askUserOptions(map[string]any{"options": []any{"Go", "Rust"}})
	require.Equal(t, []string{"Go", "Rust"}, opts)
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
	})
	require.Contains(t, desc, "go test ./...")
	require.Contains(t, desc, "Run tests")
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
