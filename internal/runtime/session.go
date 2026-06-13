package runtime

import (
	"context"

	"github.com/riipandi/elph/internal/prompt"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"go.jetify.com/typeid/v2"
)

// Session binds a coding-agent runtime to a single interactive session.
type Session struct {
	ID              typeid.TypeID
	WorkDir         string
	SystemPrompt    string
	LogPath         string
	RequestsLogPath string
	Provider        provider.Provider
	Model           string
	ProviderID      string
}

// NewSession creates a session with a generated typeid and assembled system prompt.
func NewSession(workDir string) Session {
	id := typeid.MustGenerate("sess")
	logPath, _ := OpenLog(workDir, id)
	cfg := ai.ResolveProvider()

	model := cfg.Model
	if model == "" {
		model = "Claude Sonnet 4.6"
	}
	providerID := cfg.ID
	if providerID == "" {
		providerID = "placeholder"
	}

	return Session{
		ID:              id,
		WorkDir:         workDir,
		SystemPrompt:    prompt.Build(prompt.Options{WorkDir: workDir}),
		LogPath:         logPath,
		RequestsLogPath: RequestsLogPath(workDir, id),
		Provider:        cfg.Provider,
		Model:           model,
		ProviderID:      providerID,
	}
}

// AppendLog records an event in the session log file.
func (s Session) AppendLog(kind, text string) {
	_ = AppendLog(s.LogPath, kind, text)
}

// StartTurn starts an agent turn and streams framework-neutral events.
func (s Session) StartTurn(ctx context.Context, userPrompt string) <-chan agent.Event {
	return agent.RunTurn(ctx, agent.TurnOptions{
		SystemPrompt: s.SystemPrompt,
		UserPrompt:   userPrompt,
		Model:        s.Model,
		Provider:     s.Provider,
	})
}
