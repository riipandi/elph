package renderer

import "fmt"

func formatTokenCount(tokens int) string {
	if tokens <= 0 {
		return "—"
	}
	if tokens >= 1_000_000 {
		if tokens%1_000_000 == 0 {
			return fmt.Sprintf("%dM", tokens/1_000_000)
		}
		return fmt.Sprintf("%.1fM", float64(tokens)/1_000_000)
	}
	if tokens >= 1000 {
		if tokens%1000 == 0 {
			return fmt.Sprintf("%dk", tokens/1000)
		}
		return fmt.Sprintf("%.0fk", float64(tokens)/1000)
	}
	return fmt.Sprintf("%d", tokens)
}

func (m Model) contextWindowLabel() string {
	return formatTokenCount(m.contextWindow)
}
