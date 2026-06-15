package renderer

import (
	"context"
	"encoding/json"
	"fmt"
	"image/color"
	"regexp"
	"strconv"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tools"
)

type toolInteractOffer struct {
	Req        agent.ToolInteractRequest
	RespCh     chan<- agent.ToolInteractResponse
	FromMarkup bool
}

type toolInteractOfferMsg struct {
	offer toolInteractOffer
}

type toolInteractBridge struct {
	inbox               chan toolInteractOffer
	skipSessionApproval bool // set after "allow for session" within the current turn
	deniedApprovals     map[string]struct{}
}

const (
	approvalChoiceOnce    = "once"
	approvalChoiceSession = "session"
	approvalChoiceDeny    = "deny"

	// Cap description lines so huh does not wrap unbounded text and blow past the terminal.
	maxApprovalDescriptionLines = 6
)

func newToolInteractBridge() *toolInteractBridge {
	return &toolInteractBridge{inbox: make(chan toolInteractOffer, 1)}
}

func (b *toolInteractBridge) Interact(ctx context.Context, req agent.ToolInteractRequest) (agent.ToolInteractResponse, error) {
	if b.skipSessionApproval && req.Kind == agent.ToolInteractApproval {
		return agent.ToolInteractResponse{Approved: true}, nil
	}
	if req.Kind == agent.ToolInteractApproval && b.deniedApprovals != nil {
		if _, denied := b.deniedApprovals[toolApprovalSignature(req)]; denied {
			return agent.ToolInteractResponse{Approved: false}, nil
		}
	}
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
	return inputContentWidth(borderedChromeWidth(m.chromeOuterWidth()))
}

func (m Model) offerToolInteract(msg toolInteractOfferMsg) (Model, tea.Cmd) {
	m.input.Blur()
	m.showPromptPrefix = true
	m.toolInteractPending = msg.offer
	m.toolInteractForm = newToolInteractForm(msg.offer.Req, m.toolInteractFormWidth())
	var cmds []tea.Cmd
	if init := m.toolInteractForm.Init(); init != nil {
		cmds = append(cmds, init)
	}
	cmds = append(cmds, m.activityTickCmds()...)
	return m, tea.Batch(cmds...)
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

func toolInteractHuhTheme(isDark bool) *huh.Styles {
	t := huh.ThemeBase(isDark)

	plain := lipgloss.NewStyle()
	t.Form.Base = plain
	t.Group.Base = plain
	t.Focused.Base = plain
	t.Blurred.Base = plain
	t.Focused.Card = plain
	t.Blurred.Card = plain

	t.Focused.Title = lipgloss.NewStyle().Foreground(constants.BrightText).Bold(true)
	t.Focused.Description = lipgloss.NewStyle().Foreground(constants.DimText)
	t.Focused.SelectSelector = lipgloss.NewStyle().Foreground(constants.Yellow).SetString("› ")
	t.Focused.SelectedOption = lipgloss.NewStyle().Foreground(constants.BrightText).Bold(true)
	t.Focused.UnselectedOption = lipgloss.NewStyle().Foreground(constants.DimText)
	t.Focused.TextInput.Prompt = lipgloss.NewStyle().Foreground(constants.Yellow).SetString("› ")
	t.Focused.TextInput.Text = lipgloss.NewStyle().Foreground(constants.BrightText)
	t.Focused.TextInput.Placeholder = lipgloss.NewStyle().Foreground(constants.DimText)
	t.Focused.TextInput.Cursor = lipgloss.NewStyle().Foreground(constants.Yellow)

	button := lipgloss.NewStyle().Padding(0, 1).MarginRight(1)
	t.Focused.FocusedButton = button.Foreground(constants.BrightText).Background(constants.Yellow).Bold(true)
	t.Focused.BlurredButton = button.Foreground(constants.DimText)

	t.Blurred = t.Focused
	t.Blurred.SelectSelector = lipgloss.NewStyle().SetString("  ")
	t.Group.Title = t.Focused.Title
	t.Group.Description = t.Focused.Description
	return t
}

func toolInteractFormTheme() huh.ThemeFunc {
	return huh.ThemeFunc(toolInteractHuhTheme)
}

func newAskUserForm(req agent.ToolInteractRequest, width int) *huh.Form {
	fields := parseAskUserArgs(req.Args)
	question := fields.question
	options := fields.options

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
					Title(question).
					Options(opts...).
					Value(&selected),
			),
		).
			WithWidth(width).
			WithShowHelp(false).
			WithTheme(toolInteractFormTheme())
	}

	var answer string
	return huh.NewForm(
		huh.NewGroup(
			huh.NewInput().
				Key("answer").
				Title(question).
				Placeholder("Your answer…").
				Value(&answer),
		),
	).
		WithWidth(width).
		WithShowHelp(false).
		WithTheme(toolInteractFormTheme())
}

func newToolApprovalForm(req agent.ToolInteractRequest, width int) *huh.Form {
	name, _ := tools.ResolveName(req.Name)
	choice := approvalChoiceOnce
	return huh.NewForm(
		huh.NewGroup(
			huh.NewSelect[string]().
				Key("approval").
				Title(fmt.Sprintf("Allow %s?", name)).
				Description(formatApprovalDescription(name, req.Args, width)).
				Options(
					huh.NewOption("Allow once", approvalChoiceOnce),
					huh.NewOption("Allow for session", approvalChoiceSession),
					huh.NewOption("Deny", approvalChoiceDeny),
				).
				Value(&choice),
		),
	).
		WithWidth(width).
		WithShowHelp(false).
		WithTheme(toolInteractFormTheme())
}

func formatApprovalDescription(name string, args map[string]any, width int) string {
	var b strings.Builder
	switch name {
	case tools.Bash:
		if cmd, ok := stringArgAny(args, "command"); ok {
			b.WriteString(cmd)
		}
		if desc, ok := stringArgAny(args, "description"); ok {
			if b.Len() > 0 {
				b.WriteString("\n\n")
			}
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
	return clampMultilineText(strings.TrimSpace(b.String()), width, maxApprovalDescriptionLines)
}

func clampMultilineText(text string, width, maxLines int) string {
	text = strings.TrimSpace(text)
	if text == "" {
		return ""
	}
	if maxLines <= 0 {
		maxLines = 1
	}

	paragraphs := strings.Split(text, "\n")
	var lines []string
	for pi, para := range paragraphs {
		para = strings.TrimSpace(para)
		if para == "" {
			if len(lines) > 0 && lines[len(lines)-1] != "" {
				lines = append(lines, "")
			}
			continue
		}
		wrapped := para
		if width > 0 {
			wrapped = ansi.Hardwrap(ansi.Wordwrap(para, width, ""), width, false)
		}
		wrappedLines := strings.Split(wrapped, "\n")
		for i, line := range wrappedLines {
			lines = append(lines, line)
			if len(lines) >= maxLines {
				more := i < len(wrappedLines)-1 || pi < len(paragraphs)-1
				return truncateApprovalLines(lines, maxLines, width, more)
			}
		}
	}
	return strings.Join(lines, "\n")
}

func truncateApprovalLines(lines []string, maxLines, width int, more bool) string {
	if len(lines) > maxLines {
		lines = lines[:maxLines]
		more = true
	}
	if !more {
		return strings.Join(lines, "\n")
	}
	last := lines[maxLines-1]
	if width > 0 {
		last = ansi.Truncate(last, max(1, width-1), "…")
	} else if !strings.HasSuffix(last, "…") {
		last += "…"
	}
	lines[maxLines-1] = last
	return strings.Join(lines, "\n")
}

func (m Model) updateToolInteractForm(msg tea.Msg) (Model, tea.Cmd) {
	if m, tickCmds, ok := m.handleActivityTick(msg); ok {
		return m, tea.Batch(tickCmds...)
	}

	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.toolInteractForm = m.toolInteractForm.WithWidth(m.toolInteractFormWidth())
		m = m.syncLayout(false)

	case tea.KeyPressMsg:
		if resp, ok := m.toolInteractShortcutResponse(msg); ok {
			return m.completeToolInteractWith(resp)
		}
	}

	form, cmd := m.toolInteractForm.Update(msg)
	if f, ok := form.(*huh.Form); ok {
		m.toolInteractForm = f
	}
	if cmd != nil {
		cmds = append(cmds, cmd)
	}
	m = m.syncLayout(m.content.AtBottom())

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

func (m Model) toolInteractShortcutResponse(msg tea.KeyPressMsg) (agent.ToolInteractResponse, bool) {
	req := m.toolInteractPending.Req
	switch req.Kind {
	case agent.ToolInteractApproval:
		switch strings.ToLower(msg.String()) {
		case "y":
			return agent.ToolInteractResponse{Approved: true}, true
		case "a":
			return agent.ToolInteractResponse{Approved: true, AllowSession: true}, true
		case "n":
			return agent.ToolInteractResponse{Approved: false}, true
		case "1":
			return agent.ToolInteractResponse{Approved: true}, true
		case "2":
			return agent.ToolInteractResponse{Approved: true, AllowSession: true}, true
		case "3":
			return agent.ToolInteractResponse{Approved: false}, true
		}
	case agent.ToolInteractAskUser:
		opts := parseAskUserArgs(req.Args).options
		if len(opts) > 0 && len(msg.Text) == 1 {
			if n, err := strconv.Atoi(msg.Text); err == nil && n >= 1 && n <= len(opts) {
				return agent.ToolInteractResponse{Answer: opts[n-1]}, true
			}
		}
	}
	return agent.ToolInteractResponse{}, false
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
		resp = m.askUserFormResponse(form)
	case agent.ToolInteractApproval:
		resp = m.approvalFormResponse(form)
	}
	if offer.FromMarkup && offer.Req.Kind == agent.ToolInteractAskUser {
		return m.completeMarkupAskUser(offer.Req, resp)
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
			bridge.skipSessionApproval = true
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
		return agent.ToolInteractResponse{Approved: false}
	}
	switch parseApprovalChoice(form) {
	case approvalChoiceSession:
		return agent.ToolInteractResponse{Approved: true, AllowSession: true}
	case approvalChoiceDeny:
		return agent.ToolInteractResponse{Approved: false}
	default:
		return agent.ToolInteractResponse{Approved: true}
	}
}

func parseApprovalChoice(form *huh.Form) string {
	raw := strings.TrimSpace(form.GetString("approval"))
	if raw == "" {
		if v := form.Get("approval"); v != nil {
			raw = strings.TrimSpace(fmt.Sprint(v))
		}
	}
	return normalizeApprovalChoice(raw)
}

func toolApprovalSignature(req agent.ToolInteractRequest) string {
	name, ok := tools.ResolveName(req.Name)
	if !ok {
		name = req.Name
	}
	var b strings.Builder
	b.WriteString(name)
	if name == tools.Bash {
		if cmd, ok := bashCommandArg(req.Args); ok {
			b.WriteByte(0)
			b.WriteString(cmd)
		}
		return b.String()
	}
	for _, key := range sortedArgKeys(req.Args) {
		if val, ok := stringArgAny(req.Args, key); ok {
			b.WriteByte(0)
			b.WriteString(key)
			b.WriteByte('=')
			b.WriteString(val)
		}
	}
	return b.String()
}

func (m Model) recordToolApprovalDenial(resp agent.ToolInteractResponse, req agent.ToolInteractRequest) Model {
	if req.Kind != agent.ToolInteractApproval || resp.Approved || resp.Cancelled {
		return m
	}
	bridge := m.agent.ToolInteractBridge
	if bridge == nil {
		return m
	}
	if bridge.deniedApprovals == nil {
		bridge.deniedApprovals = make(map[string]struct{})
	}
	bridge.deniedApprovals[toolApprovalSignature(req)] = struct{}{}
	return m
}

func normalizeApprovalChoice(raw string) string {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case approvalChoiceOnce, "allow once":
		return approvalChoiceOnce
	case approvalChoiceSession, "allow for session":
		return approvalChoiceSession
	case approvalChoiceDeny:
		return approvalChoiceDeny
	default:
		if raw == "" {
			return approvalChoiceOnce
		}
		return raw
	}
}

func trimTrailingLineSpaces(s string) string {
	if s == "" {
		return s
	}
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		lines[i] = strings.TrimRight(line, " ")
	}
	return strings.Join(lines, "\n")
}

func (m Model) toolInteractDialogBody() string {
	formView := trimTrailingLineSpaces(strings.TrimSuffix(m.toolInteractForm.View(), "\n\n"))
	req := m.toolInteractPending.Req

	label, accent := toolInteractDialogAccent(req)
	labelLine := lipgloss.NewStyle().Foreground(accent).Bold(true).Render(label)
	hintLine := lipgloss.NewStyle().Foreground(constants.DimText).Render(toolInteractFooterHint(req))
	return lipgloss.JoinVertical(lipgloss.Left, labelLine, "", formView, "", hintLine)
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

func toolInteractDialogAccent(req agent.ToolInteractRequest) (string, color.Color) {
	switch req.Kind {
	case agent.ToolInteractAskUser:
		return "Question", constants.Yellow
	case agent.ToolInteractApproval:
		name, _ := tools.ResolveName(req.Name)
		return fmt.Sprintf("Approve %s", name), constants.Blue
	default:
		return "Input required", constants.Blue
	}
}

func toolInteractFooterHint(req agent.ToolInteractRequest) string {
	switch req.Kind {
	case agent.ToolInteractAskUser:
		if len(parseAskUserArgs(req.Args).options) > 0 {
			return "↑/↓ · 1-9 · Enter · Esc"
		}
		return "Enter · Esc"
	case agent.ToolInteractApproval:
		return "y once · a session · n deny · 1-3 · ↑/↓ · Enter · Esc"
	default:
		return "Enter · Esc"
	}
}

const defaultAskUserQuestion = "The agent has a question for you."

type askUserFields struct {
	question string
	options  []string
}

var askUserQuotedStrings = regexp.MustCompile(`"([^"]+)"`)

func parseAskUserArgs(args map[string]any) askUserFields {
	var out askUserFields
	if args == nil {
		out.question = defaultAskUserQuestion
		return out
	}

	out.question = askUserQuestionText(args)
	out.options = askUserOptions(args)
	out.question, out.options = reconcileSwappedAskUserFields(out.question, out.options, args)

	if out.question != "" && len(out.options) == 0 {
		if opts, ok := parseJSONStringArray(out.question); ok {
			out.options = opts
			out.question = ""
		} else if opts := salvageQuotedStrings(out.question); len(opts) > 0 && strings.HasPrefix(strings.TrimSpace(out.question), "[") {
			out.options = opts
			out.question = ""
		}
	}

	if len(out.options) == 0 {
		if raw, ok := args["question"]; ok {
			out.options = askUserOptions(map[string]any{"options": raw})
			if len(out.options) > 0 {
				out.question = ""
			}
		}
	}

	if strings.TrimSpace(out.question) == "" {
		if len(out.options) > 0 {
			out.question = "Choose an option:"
		} else {
			out.question = defaultAskUserQuestion
		}
	}
	return out
}

func askUserQuestionText(args map[string]any) string {
	raw, ok := args["question"]
	if !ok || raw == nil {
		if r, ok := stringArgAny(args, "reason"); ok {
			return r
		}
		return ""
	}
	switch v := raw.(type) {
	case string:
		return strings.TrimSpace(v)
	case []any, []string:
		return ""
	default:
		s := strings.TrimSpace(fmt.Sprint(v))
		if strings.HasPrefix(s, "[") {
			return s
		}
		return s
	}
}

func reconcileSwappedAskUserFields(question string, options []string, args map[string]any) (string, []string) {
	if len(options) > 0 || strings.TrimSpace(question) == "" {
		return question, options
	}
	raw, ok := args["options"]
	if !ok || raw == nil {
		return question, options
	}
	optText, ok := raw.(string)
	if !ok {
		return question, options
	}
	optText = strings.TrimSpace(optText)
	if optText == "" || strings.HasPrefix(optText, "[") {
		return question, options
	}
	if !strings.HasPrefix(strings.TrimSpace(question), "[") {
		return question, options
	}
	opts := options
	if len(opts) == 0 {
		if parsed, ok := parseJSONStringArray(question); ok {
			opts = parsed
		} else {
			opts = salvageQuotedStrings(question)
		}
	}
	if len(opts) == 0 {
		return question, options
	}
	return optText, opts
}

func parseJSONStringArray(s string) ([]string, bool) {
	s = strings.TrimSpace(s)
	if !strings.HasPrefix(s, "[") {
		return nil, false
	}
	var opts []string
	if err := json.Unmarshal([]byte(s), &opts); err != nil {
		return nil, false
	}
	opts = trimStrings(opts)
	if len(opts) == 0 {
		return nil, false
	}
	return opts, true
}

func salvageQuotedStrings(s string) []string {
	matches := askUserQuotedStrings.FindAllStringSubmatch(s, -1)
	out := make([]string, 0, len(matches)+1)
	for _, m := range matches {
		if len(m) > 1 {
			out = append(out, strings.TrimSpace(m[1]))
		}
	}
	if tail := salvageTrailingArrayToken(s); tail != "" {
		out = append(out, tail)
	}
	return trimStrings(out)
}

func salvageTrailingArrayToken(s string) string {
	s = strings.TrimSpace(s)
	if !strings.HasPrefix(s, "[") {
		return ""
	}
	i := strings.LastIndex(s, ",")
	if i < 0 {
		return ""
	}
	tail := strings.TrimSpace(s[i+1:])
	if strings.HasSuffix(tail, `"`) {
		return ""
	}
	tail = strings.TrimPrefix(tail, `"`)
	tail = strings.TrimSuffix(tail, "]")
	return strings.TrimSpace(tail)
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
	case string:
		if opts, ok := parseJSONStringArray(v); ok {
			return opts
		}
		return nil
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
