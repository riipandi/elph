package runtime

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/pkg/core/agent"
	"go.jetify.com/typeid/v2"
)

// Session binds a coding-agent runtime to a single interactive session.
type Session struct {
	ID typeid.TypeID
}

// NewSession creates a session with a generated typeid.
func NewSession() Session {
	return Session{ID: typeid.MustGenerate("sess")}
}

// RunTurn starts an agent turn for the given user prompt.
func (s Session) RunTurn(prompt string) tea.Cmd {
	return agent.RunTurn(prompt)
}