package renderer

import (
	"math/rand"
	"os"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/config"
	"github.com/riipandi/elph/internal/constants"
)

// ─── Braille Logo ────────────────────────────────────────────────────────────

const (
	logoLine1 = "\u28FF\u28FF\u285F\u28FF\u285F\u28FF\u28FF"
	logoLine2 = "\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF"
)

// ─── Tips ────────────────────────────────────────────────────────────────────

var tips = []string{
	"Use --no-session for ephemeral mode — no session file is saved, useful for one-off queries.",
	"Send /changelog to show version history.",
	"Use /help to see all available commands.",
	"Press Ctrl+C once to cancel, twice to exit.",
	"Type :q and press Enter to quit (vim-style exit).",
	"Press Ctrl+D to exit the application.",
	"Use Tab and Shift+Tab to switch between agent modes.",
}

func randomTip() string {
	return tips[rand.Intn(len(tips))]
}

// ─── Mode Ordering ───────────────────────────────────────────────────────────

var modes = []constants.AgentMode{
	constants.ModeBuild,
	constants.ModePlan,
	constants.ModeAsk,
	constants.ModeBrave,
}

func nextMode(m constants.AgentMode) constants.AgentMode {
	for i, mode := range modes {
		if mode == m {
			return modes[(i+1)%len(modes)]
		}
	}
	return modes[0]
}

func prevMode(m constants.AgentMode) constants.AgentMode {
	for i, mode := range modes {
		if mode == m {
			prev := i - 1
			if prev < 0 {
				prev = len(modes) - 1
			}
			return modes[prev]
		}
	}
	return modes[len(modes)-1]
}

// ─── Exit Messages ──────────────────────────────────────────────────────────

const doubleTapTimeout = 3 * time.Second

type ctrlCResetMsg struct{}

// ─── Model ───────────────────────────────────────────────────────────────────

type Model struct {
	ready         bool
	width         int
	height        int
	input         textinput.Model
	vp            viewport.Model
	messages      []string
	modelName     string
	mode          constants.AgentMode
	sessionID     string
	workDir       string
	branch        string
	tip           string
	quitting      bool
	ctrlCPress    int // 0=none, 1=first, 2=second (input cleared)
	ctrlCNoticeID int // index in messages of the notice (-1 = none)
}

func New() Model {
	wd, _ := os.Getwd()

	ti := textinput.New()
	ti.Placeholder = "Type a message or /command..."
	ti.Focus()
	ti.CharLimit = 4096
	ti.Width = 60
	ti.PromptStyle = lipgloss.NewStyle().Foreground(highlight)
	ti.TextStyle = lipgloss.NewStyle()

	return Model{
		input:         ti,
		modelName:     config.AppName,
		mode:          constants.ModeBuild,
		sessionID:     "sess_abc123",
		workDir:       wd,
		branch:        "main",
		messages:      []string{},
		tip:           randomTip(),
		ctrlCNoticeID: -1,
	}
}

// ─── tea.Model Implementation ────────────────────────────────────────────────

func (m Model) Init() tea.Cmd {
	return textinput.Blink
}
