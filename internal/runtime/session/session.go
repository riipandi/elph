package session

import (
	"context"
	"github.com/riipandi/elph/internal/runtime/exec"
	"github.com/riipandi/elph/internal/runtime/log"
	"github.com/riipandi/elph/internal/runtime/toolresult"
	"time"

	"github.com/riipandi/elph/internal/prompt"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/skill"
	"github.com/riipandi/elph/pkg/tools/todolist"
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
	todoStore         *[]todolist.Todo        // heap pointer; stable when Model copies Session
	CompactionCount   int                     // number of times history has been compacted
	CompactionHistory []agent.CompactionEntry // history of compactions with summaries
}

// NewSession creates a session with a generated typeid and assembled system prompt.
func NewSession(workDir string) Session {
	id := typeid.MustGenerate("sess")
	logPath, _ := log.OpenLog(workDir, id)
	requestsLogPath, _ := log.OpenRequestsLog(workDir, id)
	prefs, err := settings.Load()
	if err != nil {
		prefs = settings.Settings{}
	}
	cfg := ai.ResolveProvider(prefs.ActiveProviderID(), prefs.ActiveModelID())

	modelName := cfg.ModelName
	if modelName == "" {
		modelName = "No model selected"
	}
	providerName := cfg.ProviderName
	if providerName == "" {
		providerName = "—"
	}

	todoStore := loadSessionTodos(workDir, id.String())
	return Session{
		ID:        id,
		WorkDir:   workDir,
		todoStore: &todoStore,
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
		ProviderID:        cfg.ProviderID,
		ProviderName:      providerName,
		Catalog:           cfg.Catalog,
		EnabledModelCount: cfg.Catalog.TotalEnabledModels(),
	}
}

// Todos returns the current session todo list.
func (s Session) Todos() []todolist.Todo {
	if s.todoStore == nil {
		return nil
	}
	if len(*s.todoStore) == 0 {
		return nil
	}
	out := make([]todolist.Todo, len(*s.todoStore))
	copy(out, *s.todoStore)
	return out
}

// ReplaceTodos replaces the session todo list.
func (s *Session) ReplaceTodos(todos []todolist.Todo) {
	if s.todoStore == nil {
		store := append([]todolist.Todo(nil), todos...)
		s.todoStore = &store
		return
	}
	*s.todoStore = append([]todolist.Todo(nil), todos...)
}

// ClearTodos removes all session todos and deletes any on-disk snapshot.
func (s *Session) ClearTodos() {
	if s.todoStore == nil {
		_ = log.SaveTodosSnapshot(s.WorkDir, s.ID.String(), nil)
		return
	}
	*s.todoStore = nil
	_ = log.SaveTodosSnapshot(s.WorkDir, s.ID.String(), nil)
}

// AppendLog records an event in the session log file.
func (s Session) AppendLog(kind, text string) {
	_ = log.AppendLog(s.LogPath, kind, text)
}

// AppendRequestsLog records a provider or tool trace line in the requests log.
func (s Session) AppendRequestsLog(kind, text string) {
	if s.RequestsLogPath == "" {
		s.RequestsLogPath = log.RequestsLogPath(s.WorkDir, s.ID)
	}
	_ = log.AppendLog(s.RequestsLogPath, kind, text)
}

// StartTurn starts an agent turn and streams framework-neutral events.
func (s *Session) StartTurn(ctx context.Context, opts agent.TurnOptions) <-chan agent.Event {
	ctx = skill.WithDepthHolder(ctx)
	ctx = withTodoSession(ctx, s.WorkDir, s.ID.String())
	if s.todoStore == nil {
		store := make([]todolist.Todo, 0)
		s.todoStore = &store
	}
	ctx = todolist.WithStore(ctx, s.todoStore)
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
			return toolRunResult(exec.ExecuteTool(ctx, s.WorkDir, name, args))
		}
	}
	return agent.RunTurn(ctx, opts)
}

func toolRunResult(result toolresult.ToolResult) agent.ToolRunResult {
	return agent.ToolRunResult{
		Output:    result.Output,
		Err:       result.Err,
		Cancelled: result.Cancelled,
	}
}

func (s Session) toolExecuteStream() agent.ToolExecuteStreamFunc {
	return func(ctx context.Context, call provider.ToolCall, args map[string]any, onChunk func(string)) agent.ToolRunResult {
		return toolRunResult(exec.ExecuteToolWithOutput(ctx, s.WorkDir, call.Name, args, onChunk))
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

// ApplyHistoryWithCompaction replaces history and tracks compaction metadata.
func (s *Session) ApplyHistoryWithCompaction(result agent.CompactionResult) {
	if len(result.Messages) == 0 {
		s.History = nil
		return
	}
	s.History = result.Messages
	if result.Entry != nil {
		s.CompactionCount++
		s.CompactionHistory = append(s.CompactionHistory, *result.Entry)
	}
}

func loadSessionTodos(workDir, sessionID string) []todolist.Todo {
	loaded, err := log.LoadTodos(workDir, sessionID)
	if err != nil || len(loaded) == 0 {
		return make([]todolist.Todo, 0)
	}
	return loaded
}
