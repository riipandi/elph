package runtime

import (
	"context"
	"time"

	"github.com/riipandi/elph/internal/prompt"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/skill"
	"go.jetify.com/typeid/v2"
)

// Session binds a coding-agent runtime to a single interactive session.
type Session struct {
	ID                typeid.TypeID
	WorkDir           string
	SystemPrompt      string
	LogPath           string
	RequestsLogPath   string
	Provider          provider.Provider
	ModelID           string
	ModelName         string
	ContextWindow     int
	MaxTokens         int
	ProviderID        string
	ProviderName      string
	Catalog           provider.Catalog
	EnabledModelCount int
	History           []provider.ChatMessage
}

// NewSession creates a session with a generated typeid and assembled system prompt.
func NewSession(workDir string) Session {
	id := typeid.MustGenerate("sess")
	logPath, _ := OpenLog(workDir, id)
	requestsLogPath, _ := OpenRequestsLog(workDir, id)
	prefs, err := settings.Load()
	if err != nil {
		prefs = settings.Settings{}
	}
	cfg := ai.ResolveProvider(prefs.ActiveProviderID(), prefs.ActiveModelID())

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
		ID:      id,
		WorkDir: workDir,
		SystemPrompt: prompt.Build(prompt.Options{
			WorkDir:                  workDir,
			PreferedResponseLanguage: prefs.ResponseLanguage(),
			CurrentDate:              time.Now().Format("2006-01-02"),
			AgentMode:                string(prefs.AgentMode()),
		}),
		LogPath:           logPath,
		RequestsLogPath:   requestsLogPath,
		Provider:          cfg.Provider,
		ModelID:           cfg.ModelID,
		ModelName:         modelName,
		ContextWindow:     cfg.ContextWindow,
		MaxTokens:         cfg.MaxTokens,
		ProviderID:        providerID,
		ProviderName:      providerName,
		Catalog:           cfg.Catalog,
		EnabledModelCount: cfg.Catalog.TotalEnabledModels(),
	}
}

// AppendLog records an event in the session log file.
func (s Session) AppendLog(kind, text string) {
	_ = AppendLog(s.LogPath, kind, text)
}

// AppendRequestsLog records a provider or tool trace line in the requests log.
func (s Session) AppendRequestsLog(kind, text string) {
	if s.RequestsLogPath == "" {
		s.RequestsLogPath = RequestsLogPath(s.WorkDir, s.ID)
	}
	_ = AppendLog(s.RequestsLogPath, kind, text)
}

// StartTurn starts an agent turn and streams framework-neutral events.
func (s Session) StartTurn(ctx context.Context, opts agent.TurnOptions) <-chan agent.Event {
	ctx = skill.WithDepthHolder(ctx)
	opts.SystemPrompt = s.SystemPrompt
	if opts.Model == "" {
		opts.Model = s.ModelID
	}
	if opts.Provider == nil {
		opts.Provider = s.Provider
	}
	if opts.WorkDir == "" {
		opts.WorkDir = s.WorkDir
	}
	if len(opts.Messages) == 0 && len(s.History) > 0 {
		opts.Messages = append([]provider.ChatMessage(nil), s.History...)
	}
	if s.RequestsLogPath != "" {
		opts.LogProvider = func(kind, text string) {
			s.AppendRequestsLog(kind, text)
		}
	}
	if opts.Provider != nil {
		opts.ToolsEnabled = true
		opts.ExecuteToolStream = s.toolExecuteStream()
		opts.ExecuteTool = func(ctx context.Context, name string, args map[string]any) agent.ToolRunResult {
			return toolRunResult(ExecuteTool(ctx, s.WorkDir, name, args))
		}
	}
	return agent.RunTurn(ctx, opts)
}

func toolRunResult(result ToolResult) agent.ToolRunResult {
	return agent.ToolRunResult{
		Output:    result.Output,
		Err:       result.Err,
		Cancelled: result.Cancelled,
	}
}

func (s Session) toolExecuteStream() agent.ToolExecuteStreamFunc {
	return func(ctx context.Context, call provider.ToolCall, args map[string]any, onChunk func(string)) agent.ToolRunResult {
		return toolRunResult(ExecuteToolWithOutput(ctx, s.WorkDir, call.Name, args, onChunk))
	}
}

// ApplyHistory replaces the session conversation history used for provider calls.
func (s *Session) ApplyHistory(history []provider.ChatMessage) {
	if len(history) == 0 {
		s.History = nil
		return
	}
	s.History = agent.CompactMessages(history)
}
