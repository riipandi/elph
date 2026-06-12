package renderer

import (
	"math/rand"
	"os"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"go.jetify.com/typeid/v2"
)

// ─── Braille Logo ────────────────────────────────────────────────────────────

const (
	logoLine1 = "\u28FF\u28FF\u285F\u28FF\u285F\u28FF\u28FF"
	logoLine2 = "\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF"
)

func randomTip() string {
	return constants.Tips[rand.Intn(len(constants.Tips))]
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
	provider      string
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
	sid := typeid.MustGenerate("sess").String()

	ti := textinput.New()
	ti.Placeholder = "Type a message or /command..."
	ti.Focus()
	ti.CharLimit = 4096
	ti.Width = 60
	ti.PromptStyle = lipgloss.NewStyle().Foreground(highlight)
	ti.TextStyle = lipgloss.NewStyle()

	return Model{
		input:         ti,
		modelName:     "Claude Sonnet 4.6",
		provider:      "anthropic",
		mode:          constants.ModeBuild,
		sessionID:     sid,
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
