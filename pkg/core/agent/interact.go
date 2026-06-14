package agent

import (
	"context"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tool"
)

// ToolInteractKind classifies UI needed before a tool runs.
type ToolInteractKind int

const (
	ToolInteractAskUser ToolInteractKind = iota
	ToolInteractApproval
)

// ToolInteractRequest asks the host (TUI) to collect user input or approval.
type ToolInteractRequest struct {
	Kind     ToolInteractKind
	ToolCall provider.ToolCall
	Name     string
	Args     map[string]any
}

// ToolInteractResponse is the user's answer to a ToolInteractRequest.
type ToolInteractResponse struct {
	Approved  bool
	Answer    string
	Cancelled bool
}

// ToolInteractFunc blocks until the host returns a ToolInteractResponse.
type ToolInteractFunc func(ctx context.Context, req ToolInteractRequest) (ToolInteractResponse, error)

// ToolInteractKindFor reports whether and how a built-in tool needs host interaction.
func ToolInteractKindFor(name string, skipApproval bool) (ToolInteractKind, bool) {
	canonical, ok := tool.ResolveName(name)
	if !ok {
		return 0, false
	}
	switch canonical {
	case tool.AskUser:
		return ToolInteractAskUser, true
	default:
		if !skipApproval && tool.RequiresApproval(canonical) && tool.IsExecutable(canonical) {
			return ToolInteractApproval, true
		}
		return 0, false
	}
}
