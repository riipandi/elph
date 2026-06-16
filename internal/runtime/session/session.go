package session

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/riipandi/elph/internal/runtime/exec"
	"github.com/riipandi/elph/internal/runtime/log"
	"github.com/riipandi/elph/internal/runtime/toolresult"
	"github.com/riipandi/elph/internal/prompt"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/skill"
	"github.com/riipandi/elph/pkg/tools/todolist"
	"github.com/riipandi/elph/pkg/tools/goal"
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
	goalManager       *goal.Manager           // goal state
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
		goalManager: goal.NewManager(),
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
	return out
}

// GoalManager returns the session's goal manager.
func (s *Session) GoalManager() *goal.Manager {
	return s.goalManager
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
	// Ensure goal manager is in context
	if s.goalManager == nil {
		s.goalManager = goal.NewManager()
	}
	ctx = goal.WithManager(ctx, s.goalManager)

	// Build system prompt with optional goal context
	opts.SystemPrompt = s.SystemPrompt
	if snapshot := s.goalManager.GetGoal(); snapshot != nil {
		opts.SystemPrompt += formatGoalPrompt(snapshot)
	}

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
		// Record goal turn progress when a goal is active
		opts.RecordGoalTurn = func(tokens int) {
			if s.goalManager != nil {
				s.goalManager.RecordTurn(tokens)
			}
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


func formatGoalPrompt(s *goal.Snapshot) string {
	var b strings.Builder
	b.WriteString("\n\n## Current Goal\n")
	fmt.Fprintf(&b, "Objective: %s\n", s.Objective)
	if s.CompletionCriterion != "" {
		fmt.Fprintf(&b, "Completion criterion: %s\n", s.CompletionCriterion)
	}
	fmt.Fprintf(&b, "Status: %s\n", s.Status)
	fmt.Fprintf(&b, "Turns used: %d\n", s.TurnsUsed)
	fmt.Fprintf(&b, "Tokens used: %d\n", s.TokensUsed)
	if s.WallClockMs > 0 {
		fmt.Fprintf(&b, "Elapsed: %s\n", fmtGoalDuration(s.WallClockMs))
	}
	if s.WallClockBudgetMs > 0 {
		fmt.Fprintf(&b, "Wall clock budget: %s\n", fmtGoalDuration(s.WallClockBudgetMs))
	}
	if s.TurnBudget > 0 {
		fmt.Fprintf(&b, "Turn budget: %d\n", s.TurnBudget)
	}
	if s.TokenBudget > 0 {
		fmt.Fprintf(&b, "Token budget: %d\n", s.TokenBudget)
	}
	b.WriteString("Work toward this goal. When you believe the goal is complete, call UpdateGoal with status=complete.")
	return b.String()
}

func fmtGoalDuration(ms int64) string {
	if ms < 1000 {
		return fmt.Sprintf("%dms", ms)
	}
	totalSeconds := ms / 1000
	if totalSeconds < 60 {
		return fmt.Sprintf("%ds", totalSeconds)
	}
	minutes := totalSeconds / 60
	seconds := totalSeconds % 60
	if minutes < 60 {
		return fmt.Sprintf("%dm%ds", minutes, seconds)
	}
	hours := minutes / 60
	minutes = minutes % 60
	return fmt.Sprintf("%dh%dm", hours, minutes)
}
