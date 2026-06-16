// Package goal implements Goal tool state, types, and a context-scoped store
// for CreateGoal, GetGoal, UpdateGoal, and SetGoalBudget tool execution.
//
// The goal lifecycle: active -> complete | blocked | paused -> active/resume.
// Tools are exposed to the model only when a goal exists (see exposure logic
// in pkg/tools/exposure).
package goal

import (
	"context"
	"fmt"
	"time"
)

// Status is a goal lifecycle state.
type Status string

const (
	StatusActive   Status = "active"
	StatusComplete Status = "complete"
	StatusPaused   Status = "paused"
	StatusBlocked  Status = "blocked"
)

// Snapshot is a point-in-time view of a goal.
type Snapshot struct {
	Objective           string `json:"objective"`
	CompletionCriterion string `json:"completionCriterion,omitempty"`
	Status              Status `json:"status"`
	TokenBudget         int64  `json:"tokenBudget,omitempty"`
	TurnBudget          int64  `json:"turnBudget,omitempty"`
	WallClockBudgetMs   int64  `json:"wallClockBudgetMs,omitempty"`
	TurnsUsed           int64  `json:"turnsUsed"`
	TokensUsed          int64  `json:"tokensUsed"`
	WallClockMs         int64  `json:"wallClockMs"`
	StartedAt           int64  `json:"startedAt"`
}

// BudgetLimits holds optional budget constraints for a goal.
type BudgetLimits struct {
	TokenBudget     *int64 `json:"tokenBudget,omitempty"`
	TurnBudget      *int64 `json:"turnBudget,omitempty"`
	WallClockBudget *int64 `json:"wallClockBudgetMs,omitempty"`
}

// Goal is the runtime state of a single goal.
type Goal struct {
	Objective           string
	CompletionCriterion string
	Status              Status
	TokenBudget         int64
	TurnBudget          int64
	WallClockBudgetMs   int64
	TurnsUsed           int64
	TokensUsed          int64
	StartedAt           time.Time
}

// Snapshot returns a point-in-time view of the goal.
func (g *Goal) Snapshot() Snapshot {
	var elapsedMs int64
	if !g.StartedAt.IsZero() {
		elapsedMs = time.Since(g.StartedAt).Milliseconds()
	}
	return Snapshot{
		Objective:           g.Objective,
		CompletionCriterion: g.CompletionCriterion,
		Status:              g.Status,
		TokenBudget:         g.TokenBudget,
		TurnBudget:          g.TurnBudget,
		WallClockBudgetMs:   g.WallClockBudgetMs,
		TurnsUsed:           g.TurnsUsed,
		TokensUsed:          g.TokensUsed,
		WallClockMs:         elapsedMs,
		StartedAt:           g.StartedAt.UnixMilli(),
	}
}

// String renders the goal as a human-readable summary.
func (g *Goal) String() string {
	if g == nil {
		return "No active goal."
	}
	s := g.Snapshot()
	return fmt.Sprintf(
		"Goal: %s\nStatus: %s\nTurns used: %d\nTokens used: %d",
		s.Objective, s.Status, s.TurnsUsed, s.TokensUsed,
	)
}

// managerKey is the context key for the goal store.
type managerKey struct{}

// Manager manages goal state within a session.
type Manager struct {
	goal *Goal
}

// NewManager creates an empty goal manager.
func NewManager() *Manager {
	return &Manager{}
}

// WithManager attaches a goal manager to context.
func WithManager(ctx context.Context, m *Manager) context.Context {
	return context.WithValue(ctx, managerKey{}, m)
}

// FromContext returns the goal manager from context, if any.
func FromContext(ctx context.Context) *Manager {
	if ctx == nil {
		return nil
	}
	m, _ := ctx.Value(managerKey{}).(*Manager)
	return m
}

// GetGoal returns the current goal snapshot, or nil if no goal exists.
func (m *Manager) GetGoal() *Snapshot {
	if m == nil || m.goal == nil {
		return nil
	}
	s := m.goal.Snapshot()
	return &s
}

// CreateGoal creates a new goal. Returns error if one already exists (unless replace is set).
func (m *Manager) CreateGoal(objective string, completionCriterion string, replace bool) (*Snapshot, error) {
	if m == nil {
		return nil, fmt.Errorf("goal manager unavailable")
	}
	if m.goal != nil && m.goal.Status != StatusComplete && !replace {
		return nil, fmt.Errorf("a goal is already active (status: %s); set replace=true to overwrite", m.goal.Status)
	}
	m.goal = &Goal{
		Objective:           objective,
		CompletionCriterion: completionCriterion,
		Status:              StatusActive,
		StartedAt:           time.Now(),
	}
	s := m.goal.Snapshot()
	return &s, nil
}

// UpdateGoal sets the goal status. Only valid transitions are allowed.
func (m *Manager) UpdateGoal(status Status) (*Snapshot, error) {
	if m == nil || m.goal == nil {
		return nil, fmt.Errorf("no active goal")
	}
	switch status {
	case StatusActive:
		if m.goal.Status != StatusPaused {
			return nil, fmt.Errorf("can only resume a paused goal (current: %s)", m.goal.Status)
		}
		m.goal.Status = StatusActive
	case StatusComplete, StatusBlocked, StatusPaused:
		m.goal.Status = status
	default:
		return nil, fmt.Errorf("invalid goal status: %s", status)
	}
	s := m.goal.Snapshot()
	return &s, nil
}

// RecordTurn increments the turn counter.
func (m *Manager) RecordTurn(tokens int) {
	if m == nil || m.goal == nil {
		return
	}
	m.goal.TurnsUsed++
	m.goal.TokensUsed += int64(tokens)
}

// SetBudgetLimits sets optional budget constraints.
func (m *Manager) SetBudgetLimits(limits BudgetLimits) {
	if m == nil || m.goal == nil {
		return
	}
	if limits.TokenBudget != nil {
		m.goal.TokenBudget = *limits.TokenBudget
	}
	if limits.TurnBudget != nil {
		m.goal.TurnBudget = *limits.TurnBudget
	}
	if limits.WallClockBudget != nil {
		m.goal.WallClockBudgetMs = *limits.WallClockBudget
	}
}

// Clear removes the current goal.
func (m *Manager) Clear() {
	if m != nil {
		m.goal = nil
	}
}
