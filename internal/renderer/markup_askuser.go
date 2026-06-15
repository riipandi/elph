package renderer

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tools"
)

type markupAskUserCmdMsg struct{}

func (m Model) tryQueueMarkupAskUser(call agent.ParsedToolCall) (Model, bool) {
	name := agent.SanitizeParsedToolName(call.Name)
	canonical, ok := tools.ResolveName(name)
	if !ok {
		return m, false
	}
	kind, needs := agent.ToolInteractKindFor(canonical, m.mode == constants.ModeBrave || m.agent.SessionAllowTools)
	if !needs || kind != agent.ToolInteractAskUser {
		return m, false
	}
	m.agent.MarkupAskUserPending = &markupAskUserOffer{
		Name:       canonical,
		Parameters: call.Parameters,
	}
	return m, true
}

func (m Model) markupAskUserCmd() tea.Cmd {
	if m.agent.MarkupAskUserPending == nil || m.toolInteractDialogActive() || m.agent.Busy {
		return nil
	}
	return func() tea.Msg { return markupAskUserCmdMsg{} }
}

func (m Model) handleMarkupAskUserCmd() (Model, tea.Cmd) {
	pending := m.agent.MarkupAskUserPending
	if pending == nil {
		return m, nil
	}
	m.agent.MarkupAskUserPending = nil
	return m.offerToolInteract(toolInteractOfferMsg{offer: toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractAskUser,
			Name: pending.Name,
			Args: parsedToolParamsToAny(pending.Parameters),
		},
		FromMarkup: true,
	}})
}

func parsedToolParamsToAny(params map[string]string) map[string]any {
	if len(params) == 0 {
		return nil
	}
	out := make(map[string]any, len(params))
	for key, value := range params {
		out[key] = value
	}
	return out
}

func (m Model) completeMarkupAskUser(req agent.ToolInteractRequest, resp agent.ToolInteractResponse) (Model, tea.Cmd) {
	fields := parseAskUserArgs(req.Args)
	var body strings.Builder
	body.WriteString(fields.question)
	switch {
	case resp.Cancelled:
		body.WriteString("\n\n(cancelled)")
	case strings.TrimSpace(resp.Answer) == "":
		body.WriteString("\n\n(no answer)")
	default:
		body.WriteString("\n\n")
		body.WriteString(resp.Answer)
	}
	m = m.addToolDetailMessage(req.Name, body.String())

	answer := strings.TrimSpace(resp.Answer)
	if resp.Cancelled || answer == "" || !m.hasActiveModel() {
		m = m.syncLayout(m.content.AtBottom())
		return m, nil
	}

	m = m.addUserMessage(answer)
	m = m.stopInFlightAgentTurn()
	m.showPromptPrefix = false
	m = m.beginAgentTurn()
	m = m.syncLayout(true)
	m, agentCmd := m.agentTurnCmds(answer, nil)
	m.input.Focus()
	var cmds []tea.Cmd
	if agentCmd != nil {
		cmds = append(cmds, agentCmd)
	}
	cmds = append(cmds, m.activityTickCmds()...)
	if len(cmds) == 0 {
		return m, nil
	}
	return m, tea.Batch(cmds...)
}

// stopInFlightAgentTurn cancels an active provider stream before starting a
// replacement turn (for example after answering a markup AskUser prompt).
func (m Model) stopInFlightAgentTurn() Model {
	if m.agent.Cancel != nil {
		m.agent.Cancel()
		m.agent.Cancel = nil
	}
	m.agent.Events = nil
	m.agent.ToolInteractBridge = nil
	if m.agent.Busy {
		m.agent.Busy = false
		m.agent.Activity = agent.ActivityIdle
		m.agent.SpinnerFrame = 0
		m = m.stopActivityStopwatch()
	}
	m.agent.ThinkingMsgID = -1
	m.agent.ResponseMsgID = -1
	m = m.clearStreamPrefixCache()
	return m
}
