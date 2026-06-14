package renderer

import (
	"math/rand"
	"os"
	"time"

	"charm.land/bubbles/v2/textarea"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/runtime"
	"github.com/riipandi/elph/internal/settings"
	"go.jetify.com/typeid/v2"
)

// ─── Braille Logo ────────────────────────────────────────────────────────────

const (
	logoLine1 = "\u28FF\u28FF\u285F\u28FF\u285F\u28FF\u28FF"
	logoLine2 = "\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF\u28FF"
)

// Dedicated random source for tips — avoids mutex contention on the global source.
var rng = rand.New(rand.NewSource(42))

func randomTip() string {
	return constants.Tips[rng.Intn(len(constants.Tips))]
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

type message struct {
	text           string
	kind           constants.MessageKind
	renderCache    messageRenderCache
	glamourPending bool
}

type Model struct {
	ready            bool
	width            int
	height           int
	content          viewport.Model
	input            textarea.Model
	messages         []message
	modelName        string
	provider         string
	mode             constants.AgentMode
	thinkingLevel    constants.ThinkingLevel
	sessionID        typeid.TypeID
	workDir          string
	branch           string
	tip              string
	contextUsed      float64 // 0.0 – 1.0
	contextWindow    int
	gitAdded         int
	gitDeleted       int
	promptChar       string // >, /, $, #
	showPromptPrefix bool   // show prompt prefix in input
	layout           LayoutCache
	shell            ShellState
	suggest          SuggestState
	agent            AgentState
	modelSelector    ModelSelectorState

	mouseEnabled  bool // mouse capture for viewport wheel/scroll
	selectingText bool // shift held — mouse released for terminal selection

	session runtime.Session

	quitting      bool
	ctrlCPress    int // 0=none, 1=first, 2=second (input cleared)
	ctrlCNoticeID int // index in messages of the notice (-1 = none)

	modelsSyncing   bool
	modelsSyncMsgID int // index in messages of models.dev sync status (-1 = none)

	inputPendingEsc bool // macOS ESC+backspace Option+Delete pair
}

// Shared "no background" style reused in textarea init to reduce allocations.
var noBgStyle = lipgloss.NewStyle().Background(lipgloss.NoColor{})

// noBgStyles returns textarea styles with transparent backgrounds and a static cursor.
func noBgStyles() textarea.Styles {
	blank := textarea.StyleState{
		Base:             noBgStyle,
		CursorLine:       noBgStyle,
		CursorLineNumber: noBgStyle,
		EndOfBuffer:      noBgStyle,
		LineNumber:       noBgStyle,
		Placeholder: lipgloss.NewStyle().
			Foreground(constants.DimText).
			Background(lipgloss.NoColor{}),
		Prompt: noBgStyle,
		Text:   noBgStyle,
	}
	return textarea.Styles{
		Focused: blank,
		Blurred: blank,
		Cursor: textarea.CursorStyle{
			Blink: false,
		},
	}
}

func New() Model {
	wd, _ := os.Getwd()
	prefs, err := settings.Load()
	if err != nil {
		prefs = settings.Settings{}
	}
	session := runtime.NewSession(wd)

	vp := viewport.New()
	vp.MouseWheelEnabled = true
	vp.KeyMap = contentViewportKeyMap()

	ta := textarea.New()
	ta.Placeholder = ""
	ta.Prompt = ""
	ta.CharLimit = 4096
	ta.ShowLineNumbers = false
	ta.SetHeight(1)
	// MaxHeight limits total line count in bubbles textarea; leave unset so
	// content can grow past the viewport cap (syncInputHeight).
	ta.SetStyles(noBgStyles())
	ta.KeyMap.InsertNewline.SetKeys("ctrl+j", "shift+enter")
	configureInputKeyMap(&ta)
	ta.Focus()

	return Model{
		content:          vp,
		input:            ta,
		modelName:        session.ModelName,
		provider:         session.ProviderName,
		contextWindow:    session.ContextWindow,
		mode:             prefs.AgentMode(),
		thinkingLevel:    prefs.ThinkingLevel(),
		sessionID:        session.ID,
		session:          session,
		workDir:          wd,
		branch:           "main",
		messages:         []message{},
		tip:              randomTip(),
		contextUsed:      0.0,
		promptChar:       ">",
		showPromptPrefix: false,
		mouseEnabled:     true,
		layout:           LayoutCache{ContentDirty: true},
		shell:            ShellState{ToolMsgID: -1},
		ctrlCNoticeID:    -1,
	}
}

// ─── tea.Model Implementation ────────────────────────────────────────────────

func (m Model) Init() tea.Cmd {
	return tea.Batch(enableTerminalFeatures(), checkModelsSyncDueCmd())
}

func (m Model) availableModelCount() int {
	return len(m.session.Catalog.AllModels())
}
