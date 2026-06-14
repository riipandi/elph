package renderer

import (
	"context"
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tool"
)

type toolInteractOffer struct {
	Req    agent.ToolInteractRequest
	RespCh chan<- agent.ToolInteractResponse
}

type toolInteractOfferMsg struct {
	offer toolInteractOffer
}

type toolInteractBridge struct {
	inbox chan toolInteractOffer
}

func newToolInteractBridge() *toolInteractBridge {
	// Buffered so the agent goroutine can offer a dialog before the TUI cmd
	// begins receiving on the inbox.
	return &toolInteractBridge{inbox: make(chan toolInteractOffer, 1)}
}

func (b *toolInteractBridge) Interact(ctx context.Context, req agent.ToolInteractRequest) (agent.ToolInteractResponse, error) {
	respCh := make(chan agent.ToolInteractResponse, 1)
	select {
	case b.inbox <- toolInteractOffer{Req: req, RespCh: respCh}:
	case <-ctx.Done():
		return agent.ToolInteractResponse{}, ctx.Err()
	}
	select {
	case resp := <-respCh:
		return resp, nil
	case <-ctx.Done():
		return agent.ToolInteractResponse{}, ctx.Err()
	}
}

func waitToolInteractOffer(b *toolInteractBridge) tea.Cmd {
	if b == nil {
		return nil
	}
	return func() tea.Msg {
		offer, ok := <-b.inbox
		if !ok {
			return nil
		}
		return toolInteractOfferMsg{offer: offer}
	}
}

func (m Model) toolInteractDialogActive() bool {
	return m.toolInteractForm != nil
}

func (m Model) toolInteractFormWidth() int {
	return m.modelsSyncFormWidth()
}

func (m Model) offerToolInteract(msg toolInteractOfferMsg) (Model, tea.Cmd) {
	m.input.Blur()
	m.toolInteractPending = msg.offer
	m.toolInteractForm = newToolInteractForm(msg.offer.Req, m.toolInteractFormWidth())
	return m, m.toolInteractForm.Init()
}

func newToolInteractForm(req agent.ToolInteractRequest, width int) *huh.Form {
	switch req.Kind {
	case agent.ToolInteractAskUser:
		return newAskUserForm(req, width)
	case agent.ToolInteractApproval:
		return newToolApprovalForm(req, width)
	default:
		return nil
	}
}

func newAskUserForm(req agent.ToolInteractRequest, width int) *huh.Form {
	question := askUserQuestion(req.Args)
	options := askUserOptions(req.Args)

	if len(options) > 0 {
		var selected string
		opts := make([]huh.Option[string], len(options))
		for i, opt := range options {
			opts[i] = huh.NewOption(opt, opt)
		}
		return huh.NewForm(
			huh.NewGroup(
				huh.NewSelect[string]().
					Key("answer").
					Title("AskUser").
					Description(question).
					Options(opts...).
					Value(&selected),
			),
		).
			WithWidth(width).
			WithShowHelp(true).
			WithTheme(huh.ThemeFunc(huh.ThemeCharm))
	}

	var answer string
	return huh.NewForm(
		huh.NewGroup(
			huh.NewInput().
				Key("answer").
				Title("AskUser").
				Description(question).
				Placeholder("Your answer…").
				Value(&answer),
		),
	).
		WithWidth(width).
		WithShowHelp(true).
		WithTheme(huh.ThemeFunc(huh.ThemeCharm))
}

func newToolApprovalForm(req agent.ToolInteractRequest, width int) *huh.Form {
	name, _ := tool.ResolveName(req.Name)
	var approved bool
	return huh.NewForm(
		huh.NewGroup(
			huh.NewConfirm().
				Key("approved").
				Title(fmt.Sprintf("Allow %s?", name)).
				Description(formatApprovalDescription(name, req.Args)).
				Affirmative("Allow").
				Negative("Deny").
				Value(&approved),
		),
	).
		WithWidth(width).
		WithShowHelp(true).
		WithTheme(huh.ThemeFunc(huh.ThemeCharm))
}

func formatApprovalDescription(name string, args map[string]any) string {
	var b strings.Builder
	switch name {
	case tool.Bash:
		if cmd, ok := stringArgAny(args, "command"); ok {
			b.WriteString("Command:\n")
			b.WriteString(cmd)
		}
		if desc, ok := stringArgAny(args, "description"); ok {
			b.WriteString("\n\n")
			b.WriteString(desc)
		}
	default:
		for _, key := range sortedArgKeys(args) {
			if val, ok := stringArgAny(args, key); ok {
				if b.Len() > 0 {
					b.WriteString("\n")
				}
				b.WriteString(key)
				b.WriteString(": ")
				b.WriteString(val)
			}
		}
	}
	return strings.TrimSpace(b.String())
}

func (m Model) updateToolInteractForm(msg tea.Msg) (Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.toolInteractForm = m.toolInteractForm.WithWidth(m.toolInteractFormWidth())
		m.layout.ContentDirty = true
		m = m.syncLayout(false)
	}

	form, cmd := m.toolInteractForm.Update(msg)
	if f, ok := form.(*huh.Form); ok {
		m.toolInteractForm = f
	}
	if cmd != nil {
		cmds = append(cmds, cmd)
	}

	switch m.toolInteractForm.State {
	case huh.StateCompleted, huh.StateAborted:
		var completeCmd tea.Cmd
		m, completeCmd = m.completeToolInteractForm()
		if completeCmd != nil {
			cmds = append(cmds, completeCmd)
		}
	}

	return m, tea.Batch(cmds...)
}

func (m Model) abortToolInteract(resp agent.ToolInteractResponse) Model {
	offer := m.toolInteractPending
	m.toolInteractForm = nil
	m.toolInteractPending = toolInteractOffer{}
	if offer.RespCh != nil {
		offer.RespCh <- resp
	}
	m.input.Focus()
	return m
}

func (m Model) completeToolInteractForm() (Model, tea.Cmd) {
	form := m.toolInteractForm
	offer := m.toolInteractPending
	m.toolInteractForm = nil
	m.toolInteractPending = toolInteractOffer{}
	m.input.Focus()

	resp := agent.ToolInteractResponse{}
	if offer.RespCh != nil {
		switch offer.Req.Kind {
		case agent.ToolInteractAskUser:
			resp = m.askUserFormResponse(form)
		case agent.ToolInteractApproval:
			resp = m.approvalFormResponse(form)
		}
		offer.RespCh <- resp
	}

	var cmds []tea.Cmd
	if m.agent.Busy && m.agent.ToolInteractBridge != nil {
		cmds = append(cmds, waitToolInteractOffer(m.agent.ToolInteractBridge))
	}
	return m, tea.Batch(cmds...)
}

func (m Model) askUserFormResponse(form *huh.Form) agent.ToolInteractResponse {
	if form.State == huh.StateAborted {
		return agent.ToolInteractResponse{Cancelled: true}
	}
	answer := strings.TrimSpace(form.GetString("answer"))
	if answer == "" {
		if raw := form.Get("answer"); raw != nil {
			answer = strings.TrimSpace(fmt.Sprint(raw))
		}
	}
	return agent.ToolInteractResponse{Answer: answer}
}

func (m Model) approvalFormResponse(form *huh.Form) agent.ToolInteractResponse {
	if form.State == huh.StateAborted {
		return agent.ToolInteractResponse{Cancelled: true}
	}
	return agent.ToolInteractResponse{Approved: form.GetBool("approved")}
}

func (m Model) toolInteractDialogView() string {
	formView := strings.TrimSuffix(m.toolInteractForm.View(), "\n\n")
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	border := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(constants.Blue).
		Padding(1, 2)
	return lipgloss.NewStyle().MarginTop(1).Render(
		border.Width(boxW).Render(formView),
	)
}

func (m Model) toolInteractDialogHeight() int {
	if !m.toolInteractDialogActive() {
		return 0
	}
	return lipgloss.Height(m.toolInteractDialogView())
}

func askUserQuestion(args map[string]any) string {
	if q, ok := stringArgAny(args, "question"); ok {
		return q
	}
	if r, ok := stringArgAny(args, "reason"); ok {
		return r
	}
	return "The agent has a question for you."
}

func askUserOptions(args map[string]any) []string {
	raw, ok := args["options"]
	if !ok || raw == nil {
		return nil
	}
	switch v := raw.(type) {
	case []string:
		return trimStrings(v)
	case []any:
		out := make([]string, 0, len(v))
		for _, item := range v {
			if s, ok := item.(string); ok {
				out = append(out, strings.TrimSpace(s))
			}
		}
		return trimStrings(out)
	default:
		return nil
	}
}

func stringArgAny(args map[string]any, key string) (string, bool) {
	raw, ok := args[key]
	if !ok || raw == nil {
		return "", false
	}
	switch v := raw.(type) {
	case string:
		s := strings.TrimSpace(v)
		return s, s != ""
	default:
		s := strings.TrimSpace(fmt.Sprint(v))
		return s, s != ""
	}
}

func sortedArgKeys(args map[string]any) []string {
	keys := make([]string, 0, len(args))
	for k := range args {
		keys = append(keys, k)
	}
	sortStrings(keys)
	return keys
}

func sortStrings(ss []string) {
	for i := 1; i < len(ss); i++ {
		for j := i; j > 0 && ss[j] < ss[j-1]; j-- {
			ss[j], ss[j-1] = ss[j], ss[j-1]
		}
	}
}

func trimStrings(in []string) []string {
	out := make([]string, 0, len(in))
	for _, s := range in {
		s = strings.TrimSpace(s)
		if s != "" {
			out = append(out, s)
		}
	}
	return out
}