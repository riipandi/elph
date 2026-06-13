package mention

import (
	"strings"

	"charm.land/lipgloss/v2"
)

// Entry is a file or directory that can be @-mentioned.
type Entry struct {
	Path  string
	IsDir bool
}

// FindActive returns the mention query and its start offset when the cursor is
// inside an @-mention token.
func FindActive(input string, cursor int) (query string, start int, ok bool) {
	if cursor < 0 || cursor > len(input) {
		return "", 0, false
	}

	at := -1
	for i := cursor - 1; i >= 0; i-- {
		switch input[i] {
		case '@':
			at = i
			i = -1
		case ' ', '\n', '\t', '\r':
			return "", 0, false
		}
	}
	if at < 0 {
		return "", 0, false
	}
	if at > 0 {
		switch input[at-1] {
		case ' ', '\n', '\t', '\r':
		default:
			return "", 0, false
		}
	}

	return input[at+1 : cursor], at, true
}

// MatchSuggestionIndex returns the suggestion index when query matches a listed path.
func MatchSuggestionIndex(suggestions []Entry, query string) (int, bool) {
	query = normalizeMentionQuery(query)
	for i, entry := range suggestions {
		if normalizeMentionQuery(entry.Path) == query {
			return i, true
		}
	}
	return 0, false
}

func normalizeMentionQuery(query string) string {
	return strings.TrimSuffix(strings.ToLower(strings.TrimSpace(query)), "/")
}

// Complete returns input with the active mention replaced by the selected entry.
func Complete(input string, start, cursor int, selected Entry) string {
	mention := "@" + selected.Path
	if selected.IsDir {
		mention += "/"
	}
	return input[:start] + mention + input[cursor:]
}

// Summary returns a short label for palette rows.
func Summary(entry Entry) string {
	if entry.IsDir {
		return "directory"
	}
	return "file"
}

// NameColumnWidth returns the display width of the widest path column.
func NameColumnWidth(entries []Entry) int {
	width := 0
	for _, entry := range entries {
		name := DisplayName(entry)
		if w := lipgloss.Width(name); w > width {
			width = w
		}
	}
	return width
}

// DisplayName returns the path shown in the mention palette.
func DisplayName(entry Entry) string {
	if entry.IsDir {
		return entry.Path + "/"
	}
	return entry.Path
}

// AlignedRow splits an entry into a justified path and summary.
func AlignedRow(entry Entry, nameColW int) (name, gap, summary string) {
	name = DisplayName(entry)
	gap = strings.Repeat(" ", max(nameColW-lipgloss.Width(name)+columnGap, columnGap))
	summary = Summary(entry)
	return name, gap, summary
}

const columnGap = 2