package renderer

import (
	"context"
	"strings"
	"time"

	"charm.land/bubbles/v2/stopwatch"
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/appconst"
	"github.com/riipandi/elph/internal/rendermd"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
)

const spinnerInterval = 200 * time.Millisecond

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

type spinnerTickMsg struct{}

func (m Model) showsActivity() bool {
	return m.agent.Busy || m.shell.Running
}

func (m Model) beginAgentTurn() Model {
	m.agent.Busy = true
	m.agent.Activity = agent.ActivityConnecting
	m.agent.SpinnerFrame = 0
	m.agent.ThinkingMsgID = -1
	m.agent.ResponseMsgID = -1
	m = m.resetToolCallStreamFilter()
	return m
}

func (m Model) beginShellActivity() Model {
	m.agent.Activity = agent.ActivityRunning
	m.agent.SpinnerFrame = 0
	return m
}

func (m Model) clearActivity() Model {
	if m.showsActivity() {
		return m
	}
	m = m.stopActivityStopwatch()
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0
	return m
}

func (m Model) showThinkingEnabled() bool {
	cfg, err := settings.Load()
	if err != nil {
		return true
	}
	return cfg.ShowThinkingEnabled()
}

func (m Model) thinkingTurnEnabled() bool {
	return m.showThinkingEnabled() && m.thinkingLevel != appconst.ThinkingOff
}

func (m Model) buildTurnOptions(prompt string, images []provider.ImageAttachment, bridge *toolInteractBridge) agent.TurnOptions {
	showThinking := m.thinkingTurnEnabled()
	prefs, prefErr := settings.Load()
	opts := agent.TurnOptions{
		UserPrompt:       prompt,
		UserImages:       images,
		Model:            m.session.ModelID,
		Provider:         m.session.Provider,
		ShowThinking:     showThinking,
		SkipToolApproval: m.mode == appconst.ModeBrave || m.agent.SessionAllowTools,
	}
	if prefErr == nil {
		opts.ProviderMaxRetries = prefs.ProviderMaxRetries()
		opts.ProviderDefaultTimeout = prefs.ProviderDefaultTimeout()
		opts.MaxToolIterations = prefs.ToolRoundsLimit()
		opts.AutoCompactContext = prefs.AutoCompactContextEnabled()
		opts.AutoCompactLimit = prefs.CompactLimit()
	}
	if bridge != nil {
		opts.InteractTool = bridge.Interact
	}
	model, modelOK := m.session.Catalog.Model(m.session.ProviderID, m.session.ModelID)
	if !modelOK {
		if reg, ok := m.session.Catalog.Provider(m.session.ProviderID); ok {
			opts.Compat = reg.Config.Compat
		}
		return opts
	}
	opts.Compat = model.Compat
	if !showThinking {
		return opts
	}
	budgets := map[string]int(nil)
	if prefErr == nil {
		budgets = prefs.ThinkingBudgetOverrides()
	}
	opts.Thinking = provider.ResolveThinking(model, m.thinkingLevel, budgets)
	if !opts.Thinking.Enabled && opts.Compat.ThinkingFormat == string(provider.ThinkingFormatQwen) {
		opts.Thinking = provider.ThinkingConfig{
			Enabled:        true,
			EnableThinking: true,
			ThinkingFormat: provider.ThinkingFormatQwen,
		}
	}
	return opts
}

func (m Model) agentTurnCmds(prompt string, images []provider.ImageAttachment) (Model, tea.Cmd) {
	ctx, cancel := context.WithCancel(context.Background())
	m.agent.Cancel = cancel
	bridge := newToolInteractBridge()
	bridge.ResolvedAskUsers = m.ensureResolvedAskUsers()
	m.agent.ToolInteractBridge = bridge
	if m.thinkingTurnEnabled() && m.agent.ThinkingMsgID < 0 {
		m = m.addThinkingMessage("")
		m.agent.ThinkingMsgID = len(m.messages) - 1
		m.layout.ContentDirty = true
	}
	events := m.session.StartTurn(ctx, m.buildTurnOptions(prompt, images, bridge))
	m.agent.Events = events
	return m, tea.Batch(
		waitAgentEvent(events),
		waitToolInteractOffer(bridge),
		m.spinnerTickCmd(),
		m.activityStopwatchStartCmd(),
	)
}

func (m Model) cancelAgentTurn() (Model, tea.Cmd) {
	m = m.cancelCtrlC()
	if m.toolInteractDialogActive() {
		offer := m.toolInteractPending
		cancelled := agent.ToolInteractResponse{Cancelled: true}
		if offer.FromMarkup && offer.Req.Kind == agent.ToolInteractAskUser {
			return m.completeMarkupAskUser(offer.Req, cancelled)
		}
		m = m.abortToolInteract(cancelled)
	}
	if m.agent.Cancel != nil {
		m.agent.Cancel()
		m.agent.Cancel = nil
	}
	m.agent.Events = nil
	m.agent.ToolInteractBridge = nil
	m.agent.Busy = false
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0
	m = m.stopActivityStopwatch()
	if thinkIdx := m.agent.ThinkingMsgID; thinkIdx >= 0 && thinkIdx < len(m.messages) {
		if strings.TrimSpace(m.messages[thinkIdx].text) == "" {
			m = m.removeMessageAt(thinkIdx)
			if m.agent.ResponseMsgID > thinkIdx {
				m.agent.ResponseMsgID--
			}
		}
	}
	m.agent.ThinkingMsgID = -1
	m.agent.ResponseMsgID = -1
	m = m.clearStreamPrefixCache()
	m, cmd := m.withMessage("(agent turn cancelled)")
	return m, cmd
}

func (m Model) spinnerTickCmd() tea.Cmd {
	if !m.showsActivity() && !m.modelsSyncingActive() {
		return nil
	}
	return tea.Tick(spinnerInterval, func(time.Time) tea.Msg { return spinnerTickMsg{} })
}

func (m Model) activityTickCmds() []tea.Cmd {
	if tick := m.spinnerTickCmd(); tick != nil {
		return []tea.Cmd{tick}
	}
	return nil
}

// handleActivityTick processes spinner and stopwatch messages. Overlays such as
// tool-approval dialogs route Update through a separate path; without this helper
// the spinner chain would stop until the dialog closes.
func (m Model) handleActivityTick(msg tea.Msg) (Model, []tea.Cmd, bool) {
	switch msg.(type) {
	case spinnerTickMsg:
		if m.showsActivity() || m.modelsSyncingActive() {
			m.agent.SpinnerFrame++
			if m.modelsSyncingActive() {
				m = m.refreshModelsSyncStatus()
			}
			if m.needsSpinnerContentRefresh() {
				m = m.invalidateSpinnerPreviewCaches()
				m.layout.ContentDirty = true
				m = m.syncLayout(m.content.AtBottom())
			}
			return m, m.activityTickCmds(), true
		}
		return m, nil, true
	case stopwatch.TickMsg, stopwatch.StartStopMsg, stopwatch.ResetMsg:
		var swCmd tea.Cmd
		m.agent.Stopwatch, swCmd = m.agent.Stopwatch.Update(msg)
		if swCmd != nil {
			return m, []tea.Cmd{swCmd}, true
		}
		return m, nil, true
	default:
		return m, nil, false
	}
}

func (m Model) finishAgentTurn(thinking, response string, providerErr error) (Model, tea.Cmd) {
	m.agent.Cancel = nil
	m.agent.Events = nil
	m.agent.ToolInteractBridge = nil
	m.agent.Busy = false
	m.agent.Activity = agent.ActivityIdle
	m.agent.SpinnerFrame = 0
	m = m.stopActivityStopwatch()

	if m.thinkingTurnEnabled() {
		if m.agent.ResponseMsgID >= 0 {
			respHoldback, thinkChunk := m.agent.ThinkTagFilter.Flush("")
			if strings.TrimSpace(thinkChunk) != "" {
				m = m.appendAgentThinkingDelta(thinkChunk)
			}
			if respHoldback != "" {
				idx := m.agent.ResponseMsgID
				if idx >= 0 && idx < len(m.messages) {
					m.messages[idx].text += respHoldback
					m.messages[idx].renderCache = messageRenderCache{}
				}
			}
		} else if strings.TrimSpace(response) != "" {
			var thinkChunk string
			response, thinkChunk = m.agent.ThinkTagFilter.Flush(response)
			if strings.TrimSpace(thinkChunk) != "" {
				m = m.appendAgentThinkingDelta(thinkChunk)
			}
			if extraThink, stripped := agent.ExtractThinkTags(response); strings.TrimSpace(extraThink) != "" {
				m = m.appendAgentThinkingDelta(extraThink)
				response = stripped
			}
		}
	}

	if thinkIdx := m.agent.ThinkingMsgID; thinkIdx >= 0 && thinkIdx < len(m.messages) {
		if strings.TrimSpace(m.messages[thinkIdx].text) == "" {
			if final := strings.TrimSpace(thinking); final != "" {
				m.messages[thinkIdx].text = final
				m.messages[thinkIdx].renderCache = messageRenderCache{}
			} else {
				m = m.removeMessageAt(thinkIdx)
				if m.agent.ResponseMsgID > thinkIdx {
					m.agent.ResponseMsgID--
				}
			}
		}
		if thinkIdx < len(m.messages) && strings.TrimSpace(m.messages[thinkIdx].text) != "" {
			m.session.AppendLog("thinking", m.messages[thinkIdx].text)
		}
	} else if m.showThinkingEnabled() && strings.TrimSpace(thinking) != "" {
		m = m.addThinkingMessage(thinking)
		m.session.AppendLog("thinking", thinking)
	}

	response, calls := m.finalizeAgentResponseText(response)
	m = m.recordToolCallRequests(calls)
	response = agent.StripExtractedPayloads(response, m.agent.TurnToolCalls)

	responseIdx := m.agent.ResponseMsgID
	if responseIdx >= 0 && strings.TrimSpace(response) == "" {
		clean, extra := agent.StripToolCalls(m.messages[responseIdx].text)
		m = m.recordToolCallRequests(extra)
		response = agent.StripExtractedPayloads(clean, m.agent.TurnToolCalls)
	}

	switch {
	case responseIdx >= 0 && strings.TrimSpace(response) != "":
		clean, extra := agent.StripToolCalls(response)
		m = m.recordToolCallRequests(extra)
		response = agent.StripExtractedPayloads(clean, m.agent.TurnToolCalls)
		if strings.TrimSpace(response) == "" {
			m = m.removeMessageAt(responseIdx)
			responseIdx = -1
			break
		}
		m.messages[responseIdx].text = agent.TruncateWithNotice(response, agent.MaxUIMessageBytes)
		m.messages[responseIdx].renderCache = messageRenderCache{}
		m.session.AppendLog("ai", response)
		m.layout.ContentDirty = true
	case responseIdx >= 0 && strings.TrimSpace(response) == "":
		m = m.removeMessageAt(responseIdx)
		responseIdx = -1
	case responseIdx < 0 && strings.TrimSpace(response) != "":
		response = agent.StripExtractedPayloads(response, m.agent.TurnToolCalls)
		if strings.TrimSpace(response) == "" {
			break
		}
		m = m.addAIMessage(agent.TruncateWithNotice(response, agent.MaxUIMessageBytes))
		responseIdx = len(m.messages) - 1
	}

	if providerErr != nil && !agent.ProviderCancelError(providerErr) {
		m = m.addProviderErrorDetail(providerErr)
	}

	m.agent.ThinkingMsgID = -1
	m.agent.ResponseMsgID = -1
	m.layout.StreamFlushPending = false
	m = m.clearStreamPrefixCache()
	rendermd.ResetCache()
	m.layout.ContentDirty = true
	m = m.syncLayout(true)

	if responseIdx >= 0 {
		var cmds []tea.Cmd
		m, renderCmd := m.scheduleMarkdownRender(responseIdx)
		if renderCmd != nil {
			cmds = append(cmds, renderCmd)
		}
		if askCmd := m.markupAskUserCmd(); askCmd != nil {
			cmds = append(cmds, askCmd)
		}
		if len(cmds) > 0 {
			return m, tea.Batch(cmds...)
		}
		return m, nil
	}
	if askCmd := m.markupAskUserCmd(); askCmd != nil {
		return m, askCmd
	}
	return m, nil
}

func (m Model) appendAgentThinkingDelta(delta string) Model {
	if delta == "" || !m.thinkingTurnEnabled() {
		return m
	}
	if m.agent.ThinkingMsgID < 0 {
		m = m.addThinkingMessage(delta)
		m.agent.ThinkingMsgID = len(m.messages) - 1
	} else {
		idx := m.agent.ThinkingMsgID
		m.messages[idx].text += delta
		m.messages[idx].renderCache = messageRenderCache{}
	}
	m = m.clearStreamPrefixCache()
	m.layout.ContentDirty = true
	return m
}

func (m Model) appendAgentResponseDelta(delta string) Model {
	if delta == "" {
		return m
	}

	if m.thinkingTurnEnabled() {
		clean, thinkChunk := m.agent.ThinkTagFilter.Process(delta)
		delta = clean
		if thinkChunk != "" {
			m = m.appendAgentThinkingDelta(thinkChunk)
		}
	}

	safe, calls := m.filterAgentResponseDelta(delta)
	m = m.recordToolCallRequests(calls)
	safe, extra := agent.StripToolCalls(safe)
	m = m.recordToolCallRequests(extra)
	if safe == "" {
		m.agent.Activity = agent.ActivityStreaming
		return m
	}

	if m.agent.ResponseMsgID < 0 {
		m.messages = append(m.messages, message{text: safe, kind: uiconst.MessageAI})
		m.agent.ResponseMsgID = len(m.messages) - 1
		m.layout.ContentDirty = true
	} else {
		idx := m.agent.ResponseMsgID
		m.messages[idx].text += safe
		m.messages[idx].renderCache = messageRenderCache{}
		m.layout.ContentDirty = true
	}
	m.agent.Activity = agent.ActivityStreaming
	return m
}
