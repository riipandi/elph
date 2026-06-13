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
	ModelID         string
	ModelName       string
	ContextWindow   int
	MaxTokens       int
	ProviderID      string
	ProviderName    string
	Catalog         provider.Catalog
}

// NewSession creates a session with a generated typeid and assembled system prompt.
func NewSession(workDir string) Session {
	id := typeid.MustGenerate("sess")
	logPath, _ := OpenLog(workDir, id)
	cfg := ai.ResolveProvider()

	modelName := cfg.ModelName
	if modelName == "" {
		modelName = "No model configured"
	}
	providerID := cfg.ProviderID
	if providerID == "" {
		providerID = "placeholder"
	}
	providerName := cfg.ProviderName
	if providerName == "" {
		providerName = providerID
	}

	return Session{
		ID:              id,
		WorkDir:         workDir,
		SystemPrompt:    prompt.Build(prompt.Options{WorkDir: workDir}),
		LogPath:         logPath,
		RequestsLogPath: RequestsLogPath(workDir, id),
		Provider:        cfg.Provider,
		ModelID:         cfg.ModelID,
		ModelName:       modelName,
		ContextWindow:   cfg.ContextWindow,
		MaxTokens:       cfg.MaxTokens,
		ProviderID:      providerID,
		ProviderName:    providerName,
		Catalog:         cfg.Catalog,
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
		Model:        s.ModelID,
		Provider:     s.Provider,
	})
}
