package toolinteract

import (
	"fmt"
	"image/color"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/riipandi/elph/pkg/tools"
)

// NewForm builds a huh form for the given tool-interact request.
func NewForm(req agent.ToolInteractRequest, width int) *huh.Form {
	switch req.Kind {
	case agent.ToolInteractAskUser:
		return newAskUserForm(req, width)
	case agent.ToolInteractApproval:
		return newToolApprovalForm(req, width)
	default:
		return nil
	}
}

// FormTheme returns the huh theme for tool-interact and similar dialogs.
func FormTheme() huh.ThemeFunc {
	return huh.ThemeFunc(toolInteractHuhTheme)
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

	t.Focused.Title = lipgloss.NewStyle().Foreground(uiconst.BrightText).Bold(true)
	t.Focused.Description = lipgloss.NewStyle().Foreground(uiconst.DimText)
	t.Focused.SelectSelector = lipgloss.NewStyle().Foreground(uiconst.Yellow).SetString("› ")
	t.Focused.SelectedOption = lipgloss.NewStyle().Foreground(uiconst.BrightText).Bold(true)
	t.Focused.UnselectedOption = lipgloss.NewStyle().Foreground(uiconst.DimText)
	t.Focused.TextInput.Prompt = lipgloss.NewStyle().Foreground(uiconst.Yellow).SetString("› ")
	t.Focused.TextInput.Text = lipgloss.NewStyle().Foreground(uiconst.BrightText)
	t.Focused.TextInput.Placeholder = lipgloss.NewStyle().Foreground(uiconst.DimText)
	t.Focused.TextInput.Cursor = lipgloss.NewStyle().Foreground(uiconst.Yellow)

	button := lipgloss.NewStyle().Padding(0, 1).MarginRight(1)
	t.Focused.FocusedButton = button.Foreground(uiconst.BrightText).Background(uiconst.Yellow).Bold(true)
	t.Focused.BlurredButton = button.Foreground(uiconst.DimText)

	t.Blurred = t.Focused
	t.Blurred.SelectSelector = lipgloss.NewStyle().SetString("  ")
	t.Group.Title = t.Focused.Title
	t.Group.Description = t.Focused.Description
	return t
}

func newAskUserForm(req agent.ToolInteractRequest, width int) *huh.Form {
	fields := ParseAskUserArgs(req.Args)
	question := fields.Question
	options := fields.Options

	if len(options) > 0 {
		var selected string
		opts := make([]huh.Option[string], len(options)+1)
		for i, opt := range options {
			opts[i] = huh.NewOption(opt, opt)
		}
		opts[len(options)] = huh.NewOption("Cancel", dialogChoiceCancel)
		selectField := huh.NewSelect[string]().
			Key("choice").
			Options(opts...).
			Value(&selected)
		var group *huh.Group
		if fields.AllowCustom {
			var custom string
			group = huh.NewGroup(
				selectField,
				huh.NewInput().
					Key("custom").
					Prompt("").
					Placeholder(AskUserCustomPlaceholder).
					Value(&custom),
			)
		} else {
			group = huh.NewGroup(selectField)
		}
		return huh.NewForm(group).
			WithWidth(width).
			WithShowHelp(false).
			WithTheme(huh.ThemeFunc(toolInteractHuhTheme))
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
		WithTheme(huh.ThemeFunc(toolInteractHuhTheme))
}

func newToolApprovalForm(req agent.ToolInteractRequest, width int) *huh.Form {
	choice := approvalChoiceOnce
	return huh.NewForm(
		huh.NewGroup(
			huh.NewSelect[string]().
				Key("approval").
				Options(
					huh.NewOption("Allow once", approvalChoiceOnce),
					huh.NewOption("Allow for session", approvalChoiceSession),
					huh.NewOption("Deny", approvalChoiceDeny),
					huh.NewOption("Cancel", dialogChoiceCancel),
				).
				Value(&choice),
		),
	).
		WithWidth(width).
		WithShowHelp(false).
		WithTheme(huh.ThemeFunc(toolInteractHuhTheme))
}

// AskUserFormResponse reads the completed ask-user form into a response.
func AskUserFormResponse(form *huh.Form) agent.ToolInteractResponse {
	if form.State == huh.StateAborted {
		return agent.ToolInteractResponse{Cancelled: true}
	}
	if IsDialogCancelChoice(formFieldString(form, "choice")) {
		return agent.ToolInteractResponse{Cancelled: true}
	}
	return agent.ToolInteractResponse{Answer: resolveAskUserFormAnswer(form)}
}

func resolveAskUserFormAnswer(form *huh.Form) string {
	return ResolveAskUserAnswer(
		formFieldString(form, "custom"),
		formFieldString(form, "choice"),
		formFieldString(form, "answer"),
	)
}

func formFieldString(form *huh.Form, key string) string {
	answer := strings.TrimSpace(form.GetString(key))
	if answer == "" {
		if raw := form.Get(key); raw != nil {
			answer = strings.TrimSpace(fmt.Sprint(raw))
		}
	}
	return answer
}

// ApprovalFormResponse reads the completed approval form into a response.
func ApprovalFormResponse(form *huh.Form) agent.ToolInteractResponse {
	if form.State == huh.StateAborted {
		return agent.ToolInteractResponse{Cancelled: true}
	}
	switch parseApprovalChoice(form) {
	case approvalChoiceSession:
		return agent.ToolInteractResponse{Approved: true, AllowSession: true}
	case approvalChoiceDeny:
		return agent.ToolInteractResponse{Approved: false}
	case dialogChoiceCancel:
		return agent.ToolInteractResponse{Cancelled: true}
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

// DialogAccent returns the dialog label and accent color for a request kind.
func DialogAccent(req agent.ToolInteractRequest) (string, color.Color) {
	switch req.Kind {
	case agent.ToolInteractAskUser:
		return "Question", uiconst.Yellow
	case agent.ToolInteractApproval:
		name, _ := tools.ResolveName(req.Name)
		return fmt.Sprintf("Approve %s", name), uiconst.Blue
	default:
		return "Input required", uiconst.Blue
	}
}

// FooterHint returns keyboard hints for the dialog footer.
func FooterHint(req agent.ToolInteractRequest) string {
	switch req.Kind {
	case agent.ToolInteractAskUser:
		fields := ParseAskUserArgs(req.Args)
		if len(fields.Options) > 0 {
			cancelNum := len(fields.Options) + 1
			if fields.AllowCustom {
				return fmt.Sprintf("↑/↓ · 1-%d · c cancel · or type below · Enter · Esc", cancelNum)
			}
			return fmt.Sprintf("↑/↓ · 1-%d · c cancel · Enter · Esc", cancelNum)
		}
		return "Enter · Esc"
	case agent.ToolInteractApproval:
		return "y once · a session · n deny · c cancel · 1-4 · ↑/↓ · Enter · Esc"
	default:
		return "Enter · Esc"
	}
}

// TrimTrailingLineSpaces removes trailing spaces from each line.
func TrimTrailingLineSpaces(s string) string {
	return trimTrailingLineSpaces(s)
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

// DialogBody renders the inner content of a tool-interact dialog.
func DialogBody(formView string, req agent.ToolInteractRequest, width int) string {
	formView = trimTrailingLineSpaces(strings.TrimSuffix(formView, "\n\n"))

	label, accent := DialogAccent(req)
	labelLine := lipgloss.NewStyle().Foreground(accent).Bold(true).Render(label)
	hintLine := lipgloss.NewStyle().Foreground(uiconst.DimText).Render(FooterHint(req))

	if req.Kind == agent.ToolInteractAskUser {
		fields := ParseAskUserArgs(req.Args)
		if len(fields.Options) > 0 {
			questionLine := lipgloss.NewStyle().
				Foreground(uiconst.BrightText).
				Width(width).
				Render(WrapAskUserQuestion(fields.Question, width))
			return lipgloss.JoinVertical(
				lipgloss.Left,
				labelLine,
				"",
				questionLine,
				"",
				formView,
				"",
				hintLine,
			)
		}
	}

	if req.Kind == agent.ToolInteractApproval {
		name, _ := tools.ResolveName(req.Name)
		promptLine := lipgloss.NewStyle().
			Foreground(uiconst.BrightText).
			Width(width).
			Render(ApprovalPromptText(name))
		parts := []string{labelLine, "", promptLine}
		if desc := FormatApprovalDescription(name, req.Args, width); desc != "" {
			descLine := lipgloss.NewStyle().
				Foreground(uiconst.DimText).
				Width(width).
				Render(desc)
			parts = append(parts, "", descLine)
		}
		parts = append(parts, "", formView, "", hintLine)
		return lipgloss.JoinVertical(lipgloss.Left, parts...)
	}

	return lipgloss.JoinVertical(lipgloss.Left, labelLine, "", formView, "", hintLine)
}

// ResizeForm updates the form width on window resize.
func ResizeForm(form *huh.Form, width int) *huh.Form {
	if form == nil {
		return nil
	}
	return form.WithWidth(width)
}

// FormCompleted reports whether the form reached a terminal state.
func FormCompleted(form *huh.Form) bool {
	if form == nil {
		return false
	}
	return form.State == huh.StateCompleted || form.State == huh.StateAborted
}

// UpdateForm forwards a message to the form and returns the updated form and cmd.
func UpdateForm(form *huh.Form, msg tea.Msg) (*huh.Form, tea.Cmd) {
	updated, cmd := form.Update(msg)
	if f, ok := updated.(*huh.Form); ok {
		return f, cmd
	}
	return form, cmd
}
