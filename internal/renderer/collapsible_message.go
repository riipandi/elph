package renderer

import (
	"image/color"
	"strings"
	"time"

	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/uiconst"
)

func isCollapsibleKind(kind uiconst.MessageKind) bool {
	return kind == uiconst.MessageDetail || kind == uiconst.MessageThinking || kind == uiconst.MessageUser
}

func messageCollapsible(msg message) bool {
	if !isCollapsibleKind(msg.kind) {
		return false
	}
	if msg.kind == uiconst.MessageUser {
		return userMessageCollapsible(msg.text)
	}
	return true
}

func collapsibleLabel(msg message) string {
	if label := strings.TrimSpace(msg.detailLabel); label != "" {
		return label
	}
	switch msg.kind {
	case uiconst.MessageThinking:
		return "Thinking"
	case uiconst.MessageUser:
		return "You"
	default:
		return "Details"
	}
}

const collapsibleHintText = "click or ctrl+o to "

func collapsibleExpandHint(expanded bool) string {
	if expanded {
		return collapsibleHintText + "collapse"
	}
	return collapsibleHintText + "expand"
}

func rowContainsCollapsibleHint(row string) bool {
	return strings.Contains(ansi.Strip(row), collapsibleHintText)
}

func detailChevron(expanded bool) string {
	if expanded {
		return "▾"
	}
	return "▸"
}

type collapsibleRenderOpts struct {
	showStatusPreview bool
	showLiveBody      bool
	spinnerFrame      int
}

func collapsiblePreview(body string, maxWidth int) string {
	line := firstDetailLine(body)
	if line == "" {
		return ""
	}
	if maxWidth <= 0 {
		return line
	}
	return ansi.Truncate(line, maxWidth, "...")
}

func collapsibleActiveLabel(kind uiconst.MessageKind, status uiconst.DetailStatus) string {
	switch kind {
	case uiconst.MessageThinking:
		return "Thinking..."
	case uiconst.MessageDetail:
		return uiconst.DetailStatusPreviewLabel(status)
	default:
		return ""
	}
}

func foregroundOnBox(box lipgloss.Style, fg color.Color) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(fg).Background(box.GetBackground())
}

func boxPaddingStyle(style lipgloss.Style, vPad, hPad, blockWidth int) lipgloss.Style {
	return lipgloss.NewStyle().
		Padding(vPad, hPad).
		Width(blockWidth).
		Background(style.GetBackground())
}

func renderOnBoxSegments(box lipgloss.Style, plain string, segments []struct {
	start, end int
	style      lipgloss.Style
}) string {
	runes := []rune(plain)
	if len(runes) == 0 {
		return ""
	}
	bg := box.GetBackground()
	var b strings.Builder
	for _, seg := range segments {
		if seg.start < 0 || seg.end > len(runes) || seg.start >= seg.end {
			continue
		}
		part := string(runes[seg.start:seg.end])
		st := seg.style.Copy().Background(bg)
		b.WriteString(st.Render(part))
	}
	return b.String()
}

func collapsibleStatusPreview(kind uiconst.MessageKind, status uiconst.DetailStatus, box lipgloss.Style, spinnerFrame, maxWidth int) string {
	label := collapsibleActiveLabel(kind, status)
	if label == "" {
		return ""
	}

	useSpinner := kind == uiconst.MessageThinking || status == uiconst.DetailStatusRunning
	if !useSpinner {
		plain := label
		if maxWidth > 0 {
			plain = ansi.Truncate(plain, maxWidth, "...")
		}
		accent := uiconst.DetailStatusAccent(status).GetForeground()
		return foregroundOnBox(box, accent).Render(plain)
	}

	frame := spinnerFrames[spinnerFrame%len(spinnerFrames)]

	var spinnerFG, labelFG color.Color
	switch kind {
	case uiconst.MessageThinking:
		spinnerFG = uiconst.Yellow
		labelFG = lipgloss.NewStyle().Foreground(uiconst.DimText).GetForeground()
	default:
		spinnerFG = uiconst.DetailStatusAccent(status).GetForeground()
		labelFG = uiconst.DetailStatusBodyStyle(status).GetForeground()
	}

	plain := frame + " " + label
	if maxWidth > 0 {
		plain = ansi.Truncate(plain, maxWidth, "...")
	}

	runes := []rune(plain)
	if len(runes) == 0 {
		return ""
	}

	spinnerStyle := foregroundOnBox(box, spinnerFG)
	labelStyle := foregroundOnBox(box, labelFG)
	spinner := spinnerStyle.Render(string(runes[0]))
	if len(runes) == 1 {
		return spinner
	}
	return spinner + labelStyle.Render(string(runes[1:]))
}

func isRunningDetailPlaceholder(body string) bool {
	trimmed := strings.TrimSpace(body)
	return trimmed == "" || trimmed == "(running...)"
}

func firstDetailLine(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if detailPreviewSkipLine(trimmed) {
			continue
		}
		return trimmed
	}
	return strings.TrimSpace(body)
}

func detailPreviewSkipLine(line string) bool {
	lower := strings.ToLower(line)
	switch lower {
	case "---", "agent prompt:", "user prompt:", "instructions":
		return true
	}
	if strings.HasPrefix(lower, "<skill_content") ||
		strings.HasPrefix(lower, "<skill_resources") ||
		strings.HasPrefix(lower, "<user_args") ||
		strings.HasPrefix(lower, "<file>") ||
		strings.HasPrefix(lower, "skill directory:") {
		return true
	}
	if strings.HasPrefix(lower, "follow the skill instructions below") {
		return true
	}
	if strings.HasPrefix(lower, "apply this skill's workflow internally") {
		return true
	}
	if strings.HasPrefix(lower, "relative paths in this skill are relative to the skill directory") {
		return true
	}
	if strings.HasPrefix(line, "#") {
		i := 0
		for i < len(line) && line[i] == '#' {
			i++
		}
		if i < len(line) && line[i] == ' ' {
			return true
		}
	}
	return false
}

func collapsibleHeaderChip(style lipgloss.Style, _ uiconst.MessageKind, label string, expanded bool) string {
	plain := detailChevron(expanded) + " " + label
	chevronFG := lipgloss.NewStyle().Foreground(uiconst.DimText).GetForeground()
	titleStyle := lipgloss.NewStyle().Foreground(style.GetForeground()).Bold(true)
	if style.GetItalic() {
		titleStyle = titleStyle.Italic(true)
	}
	body := renderOnBoxSegments(style, plain, []struct {
		start, end int
		style      lipgloss.Style
	}{
		{0, 1, lipgloss.NewStyle().Foreground(chevronFG)},
		{1, len([]rune(plain)), titleStyle},
	})
	return lipgloss.NewStyle().Padding(0, 1).Background(style.GetBackground()).Render(body)
}

func collapsibleDetailTitleLine(hPad int, status uiconst.DetailStatus, label string, expanded bool, at time.Time) string {
	plain := detailChevron(expanded) + " " + label
	runes := []rune(plain)
	if len(runes) == 0 {
		return strings.Repeat(" ", hPad)
	}
	chevron := uiconst.DetailStatusAccent(status).Render(string(runes[0]))
	var title string
	if len(runes) > 1 {
		title = lipgloss.NewStyle().Bold(true).Foreground(uiconst.DimText).Render(string(runes[1:]))
	}
	line := strings.Repeat(" ", hPad) + chevron + title
	if ts := formatMessageTimestamp(at); ts != "" {
		line += dimStyle.Render(" · " + ts)
	}
	return line
}

func collapsibleBodyBox(style lipgloss.Style, kind uiconst.MessageKind, status uiconst.DetailStatus, blockWidth, innerW, vPad, hPad int, body string, expanded bool, opts collapsibleRenderOpts) string {
	trimmed := strings.TrimSpace(body)
	if trimmed == "" && !opts.showStatusPreview {
		return ""
	}
	var content string
	preStyled := false
	switch {
	case opts.showLiveBody:
		content = body
		if kind == uiconst.MessageThinking {
			content = dimStyle.Render(body)
		}
	case opts.showStatusPreview:
		content = collapsibleStatusPreview(kind, status, style, opts.spinnerFrame, innerW)
		preStyled = true
	case expanded && trimmed != "":
		content = body
		if kind == uiconst.MessageThinking {
			content = dimStyle.Render(body)
		}
	case trimmed != "":
		if preview := collapsiblePreview(trimmed, innerW); preview != "" {
			content = dimStyle.Render(preview)
		}
	}
	if content == "" {
		return ""
	}
	if preStyled {
		return boxPaddingStyle(style, vPad, hPad, blockWidth).Render(content)
	}
	return style.Padding(vPad, hPad).Width(blockWidth).Render(content)
}

func dimItalicHintLine(hPad int, text string) string {
	return lipgloss.NewStyle().
		Foreground(uiconst.DimText).
		Italic(true).
		Background(lipgloss.NoColor{}).
		PaddingLeft(hPad).
		Render(text)
}

func collapsibleHintLine(hPad int, expanded bool) string {
	return dimItalicHintLine(hPad, collapsibleExpandHint(expanded))
}

func renderThinkingCollapsible(blockWidth int, label, body string, expanded bool, opts collapsibleRenderOpts) string {
	style := uiconst.MessageStyle(uiconst.MessageThinking).Italic(true)
	vPad, hPad := messageBlockPadding(uiconst.MessageThinking)
	innerW := max(blockWidth-2*hPad, 1)

	var b strings.Builder
	b.WriteString(collapsibleHeaderChip(style, uiconst.MessageThinking, label, expanded))
	if box := collapsibleBodyBox(style, uiconst.MessageThinking, uiconst.DetailStatusNeutral, blockWidth, innerW, vPad, hPad, body, expanded, opts); box != "" {
		b.WriteString("\n\n")
		b.WriteString(box)
	}
	b.WriteString("\n\n")
	b.WriteString(collapsibleHintLine(hPad, expanded))
	return b.String()
}

func renderDetailCollapsible(blockWidth int, label, body string, expanded bool, status uiconst.DetailStatus, at time.Time, opts collapsibleRenderOpts) string {
	style := uiconst.DetailStatusStyle(status)
	vPad, hPad := messageBlockPadding(uiconst.MessageDetail)
	innerW := max(blockWidth-2*hPad, 1)

	var b strings.Builder
	b.WriteString(collapsibleDetailTitleLine(hPad, status, label, expanded, at))
	if box := collapsibleBodyBox(style, uiconst.MessageDetail, status, blockWidth, innerW, vPad, hPad, body, expanded, opts); box != "" {
		b.WriteString("\n\n")
		b.WriteString(box)
	}

	if status != uiconst.DetailStatusRunning {
		b.WriteString("\n\n")
		b.WriteString(collapsibleHintLine(hPad, expanded))
	}

	return b.String()
}

func renderDetailMessage(blockWidth int, label, body string, expanded bool, status uiconst.DetailStatus, at time.Time, opts collapsibleRenderOpts) string {
	return renderDetailCollapsible(blockWidth, label, body, expanded, status, at, opts)
}

func renderThinkingMessage(blockWidth int, label, body string, expanded bool, opts collapsibleRenderOpts) string {
	return renderThinkingCollapsible(blockWidth, label, body, expanded, opts)
}

// renderThinkingLiveStream paints in-flight reasoning with a lightweight body
// path so token delivery does not rebuild the full collapsible chrome every flush.
func renderThinkingLiveStream(blockWidth int, label, body string, expanded bool, opts collapsibleRenderOpts) string {
	style := uiconst.MessageStyle(uiconst.MessageThinking).Italic(true)
	vPad, hPad := messageBlockPadding(uiconst.MessageThinking)
	innerW := max(blockWidth-2*hPad, 1)

	var b strings.Builder
	b.WriteString(collapsibleHeaderChip(style, uiconst.MessageThinking, label, expanded))
	if box := collapsibleBodyBox(style, uiconst.MessageThinking, uiconst.DetailStatusNeutral, blockWidth, innerW, vPad, hPad, body, expanded, opts); box != "" {
		b.WriteString("\n\n")
		b.WriteString(box)
	} else if opts.showStatusPreview {
		if preview := collapsibleStatusPreview(uiconst.MessageThinking, uiconst.DetailStatusNeutral, style, opts.spinnerFrame, innerW); preview != "" {
			b.WriteString("\n\n")
			b.WriteString(boxPaddingStyle(style, vPad, hPad, blockWidth).Render(preview))
		}
	}
	b.WriteString("\n\n")
	b.WriteString(collapsibleHintLine(hPad, expanded))
	return b.String()
}

func shellDetailLabel(command string) string {
	return "$ " + command
}

func userMessageLineCount(text string) int {
	trimmed := strings.TrimSpace(text)
	if trimmed == "" {
		return 0
	}
	return strings.Count(strings.TrimRight(text, "\n"), "\n") + 1
}

func userMessageCollapsible(text string) bool {
	return userMessageLineCount(text) > 1
}

func userMessageFooterDimStyle(bg compat.AdaptiveColor) lipgloss.Style {
	return lipgloss.NewStyle().
		Foreground(uiconst.DimText).
		Background(bg)
}

func userMessageBody(text string, expanded bool, innerW int) string {
	if expanded || !userMessageCollapsible(text) {
		return text
	}
	prefix := detailChevron(false) + " "
	preview := collapsiblePreview(text, max(innerW-len([]rune(prefix)), 1))
	return prefix + preview
}

func userMessageFooterLine(at time.Time, expanded, showHint bool, footerBg compat.AdaptiveColor) string {
	ts := formatMessageTimestamp(at)
	hint := ""
	if showHint {
		hint = collapsibleExpandHint(expanded)
	}
	dim := userMessageFooterDimStyle(footerBg)
	hintStyle := dim.Copy().Italic(true)

	switch {
	case ts == "" && hint == "":
		return ""
	case ts == "":
		return hintStyle.Render(hint)
	case hint == "":
		return dim.Render(ts)
	default:
		return dim.Render(ts) + dim.Render(" · ") + hintStyle.Render(hint)
	}
}

func renderUserCollapsible(blockWidth int, text string, expanded bool, at time.Time) string {
	vPad, hPad := messageBlockPadding(uiconst.MessageUser)
	style := uiconst.UserMessageBoxStyle()
	innerW := userBoxInnerWidth(blockWidth, hPad)
	collapsible := userMessageCollapsible(text)

	body := userMessageBody(text, expanded, innerW)
	content := body
	if footer := userMessageFooterLine(at, expanded, collapsible, uiconst.UserMsgBg); footer != "" {
		content = body + "\n\n" + footer
	}
	return renderUserBoxWithLeftBar(
		blockWidth,
		uiconst.UserMsgBg,
		uiconst.UserMsgAccent,
		style,
		vPad,
		hPad,
		content,
	)
}
