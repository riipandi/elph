package runtime

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/prompt"
	"github.com/riipandi/elph/pkg/core/agent"
	"go.jetify.com/typeid/v2"
)

// Session binds a coding-agent runtime to a single interactive session.
type Session struct {
	ID           typeid.TypeID
	WorkDir      string
	SystemPrompt string
}

// NewSession creates a session with a generated typeid and assembled system prompt.
func NewSession(workDir string) Session {
	return Session{
		ID:           typeid.MustGenerate("sess"),
		WorkDir:      workDir,
		SystemPrompt: prompt.Build(prompt.Options{WorkDir: workDir}),
	}
}

// RunTurn starts an agent turn for the given user prompt.
func (s Session) RunTurn(userPrompt string) tea.Cmd {
	return agent.RunTurn(userPrompt)
}