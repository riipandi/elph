package command

import (
	"strings"

	"charm.land/lipgloss/v2"
)

// ArgChoice describes one accepted argument for a slash command.
type ArgChoice struct {
	Value       string
	Description string
}

// ArgsHint returns a compact placeholder for command arguments.
func ArgsHint(args []ArgChoice) string {
	if len(args) == 0 {
		return ""
	}
	parts := make([]string, len(args))
	for i, arg := range args {
		parts[i] = arg.Value
	}
	return strings.Join(parts, " | ")
}

// ResolveInput splits slash input into a matched command and the argument query.
func ResolveInput(input string) (cmd SlashCommand, argQuery string, ok bool) {
	trimmed := strings.TrimLeft(input, " \t")
	if !strings.HasPrefix(trimmed, "/") {
		return SlashCommand{}, "", false
	}

	body := strings.TrimSpace(strings.TrimPrefix(trimmed, "/"))
	if body == "" {
		return SlashCommand{}, "", false
	}

	parts := strings.SplitN(body, " ", 2)
	name := strings.ToLower(parts[0])
	cmd, ok = Get(name)
	if !ok {
		return SlashCommand{}, "", false
	}

	if len(parts) == 2 {
		argQuery = strings.ToLower(strings.TrimSpace(parts[1]))
	}
	return cmd, argQuery, true
}

// SuggestArgs returns argument choices that fuzzy-match query for cmd.
func SuggestArgs(cmd SlashCommand, query string) []ArgChoice {
	query = strings.ToLower(strings.TrimSpace(query))
	if len(cmd.Args) == 0 {
		return nil
	}
	if query == "" {
		return append([]ArgChoice(nil), cmd.Args...)
	}

	out := make([]ArgChoice, 0, len(cmd.Args))
	for _, arg := range cmd.Args {
		if argScore(query, arg) >= 0 {
			out = append(out, arg)
		}
	}
	return out
}

// CompleteArgInput returns the full slash command input for the selected argument.
func CompleteArgInput(cmd SlashCommand, selected ArgChoice) string {
	return "/" + cmd.Name + " " + selected.Value
}

// ArgColumnWidth returns the display width of the widest argument value column.
func ArgColumnWidth(args []ArgChoice) int {
	width := 0
	for _, arg := range args {
		if w := lipgloss.Width(arg.Value); w > width {
			width = w
		}
	}
	return width
}

// AlignedArgRow splits an argument choice into a justified value and summary.
func AlignedArgRow(arg ArgChoice, nameColW int) (name, gap, summary string) {
	name = arg.Value
	gap = strings.Repeat(" ", max(nameColW-lipgloss.Width(name)+columnGap, columnGap))
	summary = arg.Description
	return name, gap, summary
}

// ArgChoiceIndex returns the best palette index for a partial or exact argument query.
func ArgChoiceIndex(args []ArgChoice, query string) int {
	query = strings.ToLower(strings.TrimSpace(query))
	if query == "" {
		return 0
	}

	for i, arg := range args {
		if arg.Value == query {
			return i
		}
	}

	best := 0
	bestScore := -1
	for i, arg := range args {
		if score := argScore(query, arg); score > bestScore {
			bestScore = score
			best = i
		}
	}
	if bestScore >= 0 {
		return best
	}
	return 0
}

func argScore(query string, arg ArgChoice) int {
	score := fuzzyScore(query, arg.Value)
	if desc := fuzzyScore(query, arg.Description); desc > score {
		score = desc
	}
	return score
}
