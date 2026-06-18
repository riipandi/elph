package renderer

import (
	"context"

	"charm.land/bubbles/v2/stopwatch"
	"github.com/riipandi/elph/internal/runtime/shell"
	"github.com/riipandi/elph/internal/toolinteract"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tools/todolist"
)

// ShellState tracks an in-flight shell command.
type ShellState struct {
	Running     bool
	Command     string
	Output      string
	WithContext bool
	DetailMsgID int
	Cancel      context.CancelFunc
	OutputCh    chan string
	DoneCh      chan shell.ShellResult
}

// LayoutCache stores derived layout measurements for the TUI.
type LayoutCache struct {
	InputWidth     int
	InputScrollTop int
	ChromeH        int
	ContentDirty   bool

	// StreamPrefix caches rendered messages before the active stream index so
	// only the tail message is repainted during token delivery.
	StreamPrefix          string
	StreamPrefixUpTo      int
	StreamPrefixWidth     int
	StreamPrefixBeforeLen int
	StreamPrefixDetailSig uint64
	StreamFlushPending    bool
}

// AgentState tracks agent turn progress and activity UI.
type AgentState struct {
	Activity             agent.Activity
	SpinnerFrame         int
	Stopwatch            stopwatch.Model
	ToolCallFilter       agent.ToolCallStreamFilter
	ThinkTagFilter       agent.ThinkTagStreamFilter
	TurnToolCalls        []agent.ParsedToolCall
	NativeToolMsgIDs     map[string]int
	SeenToolCalls        map[string]struct{}
	Busy                 bool
	Events               <-chan agent.Event
	ToolInteractBridge   *toolinteract.Bridge
	Cancel               context.CancelFunc
	ThinkingMsgID        int
	ResponseMsgID        int
	TodoListUpdating     bool
	TodoListBefore       []todolist.Todo
	SessionAllowTools    bool // skip approval dialogs until the TUI session ends
	MarkupAskUserPending *markupAskUserOffer
	ResolvedAskUsers     map[string]toolinteract.AskUserResolution

	// CommitWorkDir is non-empty when a /commit turn is in flight.
	// After the turn completes, the renderer pipes the response into git commit -m.
	CommitWorkDir string

	// SavedSystemPrompt holds the original system prompt before /commit overrides it.
	// Restored after the turn completes or is cancelled.
	SavedSystemPrompt string
}

type markupAskUserOffer struct {
	Name       string
	Parameters map[string]string
}
