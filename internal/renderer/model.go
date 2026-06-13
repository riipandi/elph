package renderer

import (
	"math/rand"
	"os"
	"time"

	"github.com/charmbracelet/bubbles/textarea"
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

type msgKind int

const (
	msgUser   msgKind = iota // User input message
	msgAI                    // AI response message
	msgSystem                // System/status message
)

type message struct {
	text string
	kind msgKind
}

type Model struct {
	ready         bool
	width         int
	height        int
	input         textarea.Model
	vp            viewport.Model
	messages      []message
	modelName     string
	provider      string
	mode          constants.AgentMode
	thinkingLevel constants.ThinkingLevel
	sessionID     string
	workDir       string
	branch        string
	tip           string
	contextUsed   float64 // 0.0 – 1.0
	gitAdded      int
	gitDeleted    int
	promptChar    string      // >, /, $, #

	quitting      bool
	ctrlCPress    int // 0=none, 1=first, 2=second (input cleared)
	ctrlCNoticeID int // index in messages of the notice (-1 = none)
}

func New() Model {
	wd, _ := os.Getwd()
	sid := typeid.MustGenerate("sess").String()

	ta := textarea.New()
	ta.Placeholder = "Type a message or /command..."
	ta.Prompt = ""
	ta.CharLimit = 4096
	ta.ShowLineNumbers = false
	ta.SetHeight(1)
	ta.MaxHeight = 6
	ta.FocusedStyle.Text = lipgloss.NewStyle()
	ta.BlurredStyle.Text = lipgloss.NewStyle()
	ta.KeyMap.InsertNewline.SetKeys("ctrl+j", "shift+enter")
	ta.Focus()

	return Model{
		input:          ta,
		modelName:      "Claude Sonnet 4.6",
		provider:       "anthropic",
		mode:           constants.ModeBuild,
		thinkingLevel:  constants.ThinkingHigh,
		sessionID:      sid,
		workDir:        wd,
		branch:         "main",
		messages:       []message{},
		tip:            randomTip(),
		contextUsed:    0.0,
		promptChar:     ">",

		ctrlCNoticeID:  -1,
	}
}

// ─── tea.Model Implementation ────────────────────────────────────────────────

func (m Model) Init() tea.Cmd {
	return textarea.Blink
}
