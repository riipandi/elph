package renderer

import (
	"fmt"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/atotto/clipboard"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/git"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai/provider"
)

const gitRefreshInterval = 2 * time.Minute

type gitStatusMsg struct {
	status git.Status
	light  bool // branch only; preserve cached line stats
}

func refreshGitBranchCmd(workDir string) tea.Cmd {
	return func() tea.Msg {
		return gitStatusMsg{status: git.ReadBranch(workDir), light: true}
	}
}

func refreshGitStatusCmd(workDir string) tea.Cmd {
	return func() tea.Msg {
		return gitStatusMsg{status: git.Read(workDir)}
	}
}

func gitRefreshTickCmd() tea.Cmd {
	return tea.Tick(gitRefreshInterval, func(time.Time) tea.Msg {
		return gitRefreshTickMsg{}
	})
}

type gitRefreshTickMsg struct{}

func (m Model) applyGitBranch(st git.Status) Model {
	if st.Branch != "" {
		m.branch = st.Branch
	}
	return m
}

func (m Model) applyGitStatus(st git.Status) Model {
	m = m.applyGitBranch(st)
	m.gitAdded = st.Added
	m.gitDeleted = st.Deleted
	return m
}

func (m Model) handleGitStatus(msg gitStatusMsg) Model {
	if msg.light {
		return m.applyGitBranch(msg.status)
	}
	return m.applyGitStatus(msg.status)
}

func (m Model) syncActiveModelMetadata() Model {
	model, ok := m.session.Catalog.Model(m.session.ProviderID, m.session.ModelID)
	if !ok {
		m.modelSupportsImage = false
		return m
	}
	m.modelSupportsImage = provider.SupportsImageInput(model.Input)
	m.modelCost = model.Cost
	return m
}

func (m Model) applyTurnUsage(usage provider.TurnUsage) Model {
	hasUsage := usage.InputTokens > 0 || usage.OutputTokens > 0 ||
		usage.CacheReadTokens > 0 || usage.CacheWriteTokens > 0
	if hasUsage {
		m.tokensUsed += usage.InputTokens + usage.OutputTokens +
			usage.CacheReadTokens + usage.CacheWriteTokens
		m.sessionCost += m.modelCost.TurnCostUSD(usage)
	} else {
		m.tokensUsed = m.estimatedContextTokens()
	}
	if m.contextWindow > 0 {
		m.contextUsed = min(float64(m.tokensUsed)/float64(m.contextWindow), 1.0)
	}
	return m
}

func (m Model) estimatedContextTokens() int {
	total := estimateTokens(m.session.SystemPrompt)
	for _, msg := range m.messages {
		total += estimateTokens(msg.text)
		if msg.detailLabel != "" {
			total += estimateTokens(msg.detailLabel)
		}
	}
	return total
}

func estimateTokens(text string) int {
	if text == "" {
		return 0
	}
	return (len(text) + 3) / 4
}

func (m Model) displayContextFraction() float64 {
	if m.contextWindow <= 0 {
		return 0
	}
	used := m.tokensUsed
	if used == 0 {
		used = m.estimatedContextTokens()
	}
	return min(float64(used)/float64(m.contextWindow), 1.0)
}

func (m Model) footerCostLabel() string {
	return fmt.Sprintf("$%.2f", m.sessionCost)
}

func (m Model) footerImageLabel() string {
	if m.modelSupportsImage {
		return "IMG"
	}
	return "—"
}

func (m Model) footerViewportTop() int {
	top := m.content.Height()
	if av := m.activityView(); av != "" {
		top += lipgloss.Height(av)
	}
	top += m.todoPanelHeight()
	top += lipgloss.Height(m.inputChromeView())
	return top
}

func (m Model) isInFooterArea(y int) bool {
	if !m.ready {
		return false
	}
	top := m.footerViewportTop()
	footerH := lipgloss.Height(m.footerView())
	return y >= top && y < top+footerH
}

func (m Model) cycleThinkingLevel() (Model, tea.Cmd) {
	if model, ok := m.session.Catalog.Model(m.session.ProviderID, m.session.ModelID); ok {
		m.thinkingLevel = provider.NextSupportedThinkingLevel(m.thinkingLevel, model)
	} else {
		m.thinkingLevel = constants.NextThinkingLevel(m.thinkingLevel)
	}
	_ = settings.SetThinkingLevel(m.thinkingLevel)
	return m.withMessage(fmt.Sprintf("Thinking level: %s", m.thinkingLevel))
}

func (m Model) cycleAgentMode() (Model, tea.Cmd) {
	m.mode = nextMode(m.mode)
	_ = settings.SetAgentMode(m.mode)
	return m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))
}

func (m Model) handleFooterClick(x, y int) (Model, tea.Cmd) {
	if !m.isInFooterArea(y) {
		return m, nil
	}
	zone, ok := m.footerZoneAt(x, y-m.footerViewportTop())
	if !ok {
		return m, nil
	}

	switch zone {
	case footerZoneModel:
		return m.triggerModelSelector()
	case footerZoneThinking:
		return m.cycleThinkingLevel()
	case footerZoneMode:
		return m.cycleAgentMode()
	case footerZoneWorkdir:
		_ = clipboard.WriteAll(m.workDir)
		return m.withMessage("Copied directory to clipboard")
	case footerZoneSession:
		_ = clipboard.WriteAll(m.sessionID.String())
		return m.withMessage("Copied session id to clipboard")
	case footerZoneBranch, footerZoneGit:
		m = m.applyGitStatus(git.Read(m.workDir))
		return m, nil
	default:
		return m, nil
	}
}
