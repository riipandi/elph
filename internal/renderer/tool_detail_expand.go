package renderer

import (
	"strings"

	"github.com/riipandi/elph/pkg/tool"
)

const toolDetailLongLineBytes = 120

// toolDetailExpandedByDefault chooses the initial expand state for native tool detail boxes.
// Shell output stays expanded; long non-shell content starts collapsed.
func toolDetailExpandedByDefault(label, body string) bool {
	if isShellDetailLabel(label) {
		return true
	}
	if isRunningDetailPlaceholder(body) {
		return true
	}
	return !detailContentLong(body)
}

func isShellDetailLabel(label string) bool {
	label = strings.TrimSpace(label)
	if label == tool.Bash {
		return true
	}
	return strings.HasPrefix(label, "$ ")
}

func detailContentLong(body string) bool {
	trimmed := strings.TrimSpace(body)
	if trimmed == "" {
		return false
	}
	lines := 0
	for _, line := range strings.Split(trimmed, "\n") {
		if strings.TrimSpace(line) != "" {
			lines++
		}
	}
	if lines >= 2 {
		return true
	}
	return len(trimmed) > toolDetailLongLineBytes
}
