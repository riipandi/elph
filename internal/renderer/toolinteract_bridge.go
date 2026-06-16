package renderer

import (
	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/toolinteract"
	"github.com/riipandi/elph/pkg/core/agent"
)

type toolInteractOffer = toolinteract.Offer
type toolInteractOfferMsg = toolinteract.OfferMsg
type toolInteractBridge = toolinteract.Bridge
type askUserResolution = toolinteract.AskUserResolution
type askUserFields = toolinteract.AskUserFields

const (
	defaultAskUserQuestion   = toolinteract.DefaultAskUserQuestion
	askUserCustomPlaceholder = toolinteract.AskUserCustomPlaceholder
)

func newToolInteractBridge() *toolinteract.Bridge          { return toolinteract.NewBridge() }
func waitToolInteractOffer(b *toolinteract.Bridge) tea.Cmd { return toolinteract.WaitOffer(b) }
func newToolInteractForm(req agent.ToolInteractRequest, width int) *huh.Form {
	return toolinteract.NewForm(req, width)
}
func newAskUserForm(req agent.ToolInteractRequest, width int) *huh.Form {
	return toolinteract.NewForm(req, width)
}
func newToolApprovalForm(req agent.ToolInteractRequest, width int) *huh.Form {
	return toolinteract.NewForm(req, width)
}
func parseAskUserArgs(args map[string]any) askUserFields { return toolinteract.ParseAskUserArgs(args) }
func resolveAskUserAnswer(custom, choice, fallback string) string {
	return toolinteract.ResolveAskUserAnswer(custom, choice, fallback)
}
func lookupResolvedAskUser(store *map[string]askUserResolution, req agent.ToolInteractRequest) (agent.ToolInteractResponse, bool) {
	return toolinteract.LookupResolvedAskUser(store, req)
}
func toolInteractAskUserSignature(req agent.ToolInteractRequest) string {
	return toolinteract.AskUserSignature(req)
}
func toolApprovalSignature(req agent.ToolInteractRequest) string {
	return toolinteract.ApprovalSignature(req)
}

//nolint:unused // kept for future use
func isDialogCancelChoice(raw string) bool { return toolinteract.IsDialogCancelChoice(raw) }

const (
	approvalChoiceOnce          = toolinteract.ApprovalChoiceOnce
	approvalChoiceSession       = toolinteract.ApprovalChoiceSession
	approvalChoiceDeny          = toolinteract.ApprovalChoiceDeny
	dialogChoiceCancel          = toolinteract.DialogChoiceCancel
	maxApprovalDescriptionLines = toolinteract.MaxApprovalDescriptionLines
)

func formatApprovalDescription(name string, args map[string]any, width int) string {
	return toolinteract.FormatApprovalDescription(name, args, width)
}
func clampMultilineText(text string, width, maxLines int) string {
	return toolinteract.ClampMultilineText(text, width, maxLines)
}
func normalizeApprovalChoice(raw string) string {
	return toolinteract.NormalizeApprovalChoice(raw)
}
func toolInteractFormTheme() huh.ThemeFunc { return toolinteract.FormTheme() }
func trimTrailingLineSpaces(s string) string {
	return toolinteract.TrimTrailingLineSpaces(s)
}

func (m Model) toolInteractShortcutResponse(msg tea.KeyPressMsg) (agent.ToolInteractResponse, bool) {
	return toolinteract.ShortcutResponse(m.toolInteractPending.Req, msg)
}

func (m Model) approvalFormResponse(form *huh.Form) agent.ToolInteractResponse {
	return toolinteract.ApprovalFormResponse(form)
}

//nolint:unused // kept for future use
func (m Model) askUserFormResponse(form *huh.Form) agent.ToolInteractResponse {
	return toolinteract.AskUserFormResponse(form)
}

func (m Model) toolInteractDialogActive() bool {
	return m.toolInteractForm != nil
}

func (m Model) toolInteractFormWidth() int {
	return inputContentWidth(borderedChromeWidth(m.chromeOuterWidth()))
}

func (m Model) offerToolInteract(msg toolInteractOfferMsg) (Model, tea.Cmd) {
	offer := msg.Offer
	if offer.Req.Kind == agent.ToolInteractAskUser {
		if resp, ok := m.lookupResolvedAskUser(offer.Req); ok {
			if offer.FromMarkup {
				return m, nil
			}
			if offer.RespCh != nil {
				offer.RespCh <- resp
			}
			return m.finalizeToolInteractComplete()
		}
	}
	m.input.Blur()
	m.showPromptPrefix = true
	m.toolInteractPending = offer
	m.toolInteractForm = newToolInteractForm(offer.Req, m.toolInteractFormWidth())
	var cmds []tea.Cmd
	if init := m.toolInteractForm.Init(); init != nil {
		cmds = append(cmds, init)
	}
	cmds = append(cmds, m.activityTickCmds()...)
	return m, tea.Batch(cmds...)
}

func (m Model) updateToolInteractForm(msg tea.Msg) (Model, tea.Cmd) {
	if tickM, tickCmds, ok := m.handleActivityTick(msg); ok {
		return tickM, tea.Batch(tickCmds...)
	}

	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.toolInteractForm = toolinteract.ResizeForm(m.toolInteractForm, m.toolInteractFormWidth())
		m = m.syncLayout(false)

	case tea.KeyPressMsg:
		if resp, ok := toolinteract.ShortcutResponse(m.toolInteractPending.Req, msg); ok {
			return m.completeToolInteractWith(resp)
		}
		if resp, ok := toolinteract.AskUserChoiceEnterResponse(m.toolInteractForm, m.toolInteractPending.Req, msg); ok {
			return m.completeToolInteractWith(resp)
		}
	}

	form, cmd := toolinteract.UpdateForm(m.toolInteractForm, msg)
	m.toolInteractForm = form
	if cmd != nil {
		cmds = append(cmds, cmd)
	}
	m = m.syncLayout(m.content.AtBottom())

	if toolinteract.FormCompleted(m.toolInteractForm) {
		var completeCmd tea.Cmd
		m, completeCmd = m.completeToolInteractForm()
		if completeCmd != nil {
			cmds = append(cmds, completeCmd)
		}
	}

	return m, tea.Batch(cmds...)
}

func (m Model) completeToolInteractWith(resp agent.ToolInteractResponse) (Model, tea.Cmd) {
	offer := m.toolInteractPending
	m.toolInteractForm = nil
	m.toolInteractPending = toolInteractOffer{}
	m.showPromptPrefix = false
	m.input.Focus()
	if offer.FromMarkup && offer.Req.Kind == agent.ToolInteractAskUser {
		return m.completeMarkupAskUser(offer.Req, resp)
	}
	if offer.Req.Kind == agent.ToolInteractAskUser {
		m = m.recordAskUserResolution(offer.Req, resp)
	}
	m = m.applyApprovalInteractUI(resp, offer.Req)
	m = m.applySessionToolApproval(resp)
	m = m.recordToolApprovalDenial(resp, offer.Req)

	if offer.RespCh != nil {
		offer.RespCh <- resp
	}

	return m.finalizeToolInteractComplete()
}

func (m Model) abortToolInteract(resp agent.ToolInteractResponse) Model {
	offer := m.toolInteractPending
	m.toolInteractForm = nil
	m.toolInteractPending = toolInteractOffer{}
	m.showPromptPrefix = false
	if offer.Req.Kind == agent.ToolInteractAskUser {
		m = m.recordAskUserResolution(offer.Req, resp)
	}
	if offer.RespCh != nil {
		offer.RespCh <- resp
	}
	m.input.Focus()
	return m.syncLayout(m.content.AtBottom())
}

func (m Model) completeToolInteractForm() (Model, tea.Cmd) {
	form := m.toolInteractForm
	offer := m.toolInteractPending
	m.toolInteractForm = nil
	m.toolInteractPending = toolInteractOffer{}
	m.showPromptPrefix = false
	m.input.Focus()

	resp := agent.ToolInteractResponse{}
	switch offer.Req.Kind {
	case agent.ToolInteractAskUser:
		resp = toolinteract.AskUserFormResponse(form)
	case agent.ToolInteractApproval:
		resp = toolinteract.ApprovalFormResponse(form)
	}
	if offer.FromMarkup && offer.Req.Kind == agent.ToolInteractAskUser {
		return m.completeMarkupAskUser(offer.Req, resp)
	}
	if offer.Req.Kind == agent.ToolInteractAskUser {
		m = m.recordAskUserResolution(offer.Req, resp)
	}
	if offer.RespCh != nil {
		m = m.applyApprovalInteractUI(resp, offer.Req)
		m = m.applySessionToolApproval(resp)
		m = m.recordToolApprovalDenial(resp, offer.Req)
		offer.RespCh <- resp
	}

	return m.finalizeToolInteractComplete()
}

func (m Model) applySessionToolApproval(resp agent.ToolInteractResponse) Model {
	if resp.Approved && resp.AllowSession {
		m.agent.SessionAllowTools = true
		if bridge := m.agent.ToolInteractBridge; bridge != nil {
			bridge.SkipSessionApproval = true
		}
	}
	return m
}

func (m Model) finalizeToolInteractComplete() (Model, tea.Cmd) {
	var cmds []tea.Cmd
	if m.agent.Busy && m.agent.ToolInteractBridge != nil {
		cmds = append(cmds, waitToolInteractOffer(m.agent.ToolInteractBridge))
	}
	cmds = append(cmds, m.activityTickCmds()...)
	m = m.syncLayout(m.content.AtBottom())
	return m.batchAgentDrain(cmds...)
}

func (m Model) recordToolApprovalDenial(resp agent.ToolInteractResponse, req agent.ToolInteractRequest) Model {
	toolinteract.RecordApprovalDenial(m.agent.ToolInteractBridge, resp, req)
	return m
}

func (m Model) toolInteractDialogBody() string {
	formView := m.toolInteractForm.View()
	return toolinteract.DialogBody(formView, m.toolInteractPending.Req, m.toolInteractFormWidth())
}

func (m Model) toolInteractChromeView() string {
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	inner := m.toolInteractDialogBody()
	return lipgloss.NewStyle().MarginTop(1).Render(
		cachedInputBorder(m.mode).Width(boxW).Render(inner),
	)
}

func (m Model) toolInteractDialogHeight() int {
	if !m.toolInteractDialogActive() {
		return 0
	}
	return lipgloss.Height(m.toolInteractChromeView())
}

func (m Model) ensureResolvedAskUsers() *map[string]askUserResolution {
	if m.agent.ResolvedAskUsers == nil {
		m.agent.ResolvedAskUsers = make(map[string]askUserResolution)
	}
	return &m.agent.ResolvedAskUsers
}

func (m Model) lookupResolvedAskUser(req agent.ToolInteractRequest) (agent.ToolInteractResponse, bool) {
	return lookupResolvedAskUser(m.ensureResolvedAskUsers(), req)
}

func (m Model) recordAskUserResolution(req agent.ToolInteractRequest, resp agent.ToolInteractResponse) Model {
	if req.Kind != agent.ToolInteractAskUser {
		return m
	}
	store := m.ensureResolvedAskUsers()
	toolinteract.RecordAskUserResolution(store, req, resp)
	if pending := m.agent.MarkupAskUserPending; pending != nil {
		if m.markupAskUserSignature(pending) == toolinteract.AskUserSignature(req) {
			m.agent.MarkupAskUserPending = nil
		}
	}
	return m
}

func (m Model) parsedAskUserResolved(call agent.ParsedToolCall) bool {
	return toolinteract.ParsedAskUserResolved(m.agent.ResolvedAskUsers, call)
}

func (m Model) markupAskUserSignature(offer *markupAskUserOffer) string {
	if offer == nil {
		return ""
	}
	return toolinteract.ToolCallSignature(agent.ParsedToolCall{
		Name:       offer.Name,
		Parameters: offer.Parameters,
	})
}
