package renderer

import (
	"context"

	"charm.land/bubbles/v2/stopwatch"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/mention"
	"github.com/riipandi/elph/internal/runtime"
	"github.com/riipandi/elph/pkg/core/agent"
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
	DoneCh      chan runtime.ShellResult
}

// SuggestState tracks slash-command and @-mention palettes.
type SuggestState struct {
	CmdSuggestions      []command.SlashCommand
	CmdSuggestIndex     int
	ArgSuggestions      []command.ArgChoice
	ArgSuggestIndex     int
	MentionSuggestions  []mention.Entry
	MentionSuggestIndex int
	MentionIndex        []mention.Entry
	MentionIndexDir     string
	MentionIndexLoading bool
	MentionActiveQuery  string
	MentionFilterQuery  string
	MentionUserSelected bool
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
	Activity         agent.Activity
	SpinnerFrame     int
	Stopwatch        stopwatch.Model
	ToolCallFilter   agent.ToolCallStreamFilter
	TurnToolCalls    []agent.ParsedToolCall
	NativeToolMsgIDs map[string]int
	SeenToolCalls    map[string]struct{}
	Busy             bool
	Events           <-chan agent.Event
	Cancel           context.CancelFunc
	ThinkingMsgID    int
	ResponseMsgID    int
}
