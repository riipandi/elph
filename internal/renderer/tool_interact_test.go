package renderer

import (
	"testing"

	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestAskUserQuestionAndOptions(t *testing.T) {
	q := askUserQuestion(map[string]any{"question": "Go or Rust?"})
	require.Equal(t, "Go or Rust?", q)

	opts := askUserOptions(map[string]any{"options": []any{"Go", "Rust"}})
	require.Equal(t, []string{"Go", "Rust"}, opts)
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