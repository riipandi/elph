package renderer

import (
	"context"

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
	ToolMsgID   int
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
}

// LayoutCache stores derived layout measurements for the TUI.
type LayoutCache struct {
	InputWidth     int
	InputScrollTop int
	ChromeH        int
	ContentDirty   bool
}

// AgentState tracks agent turn progress and activity UI.
type AgentState struct {
	Activity     agent.Activity
	SpinnerFrame int
	Busy         bool
	Events       <-chan agent.Event
	Cancel       context.CancelFunc
}