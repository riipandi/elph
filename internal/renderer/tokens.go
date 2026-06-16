package renderer

import (
	"fmt"

	"github.com/riipandi/elph/internal/settings"
)

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

// footerTokenUsageLabel formats the token usage label based on the display mode.
// Format: USAGE | LIMIT where USAGE is percentage or token count, LIMIT is context window.
func (m Model) footerTokenUsageLabel(ctxFrac float64, tokensUsed int) string {
	mode := settings.ParseFooterTokenDisplay(m.footerTokenDisplay)
	windowLabel := m.contextWindowLabel()

	// Sync token count with estimation when no actual usage data yet
	displayTokens := tokensUsed
	if displayTokens == 0 {
		displayTokens = m.estimatedContextTokens()
	}

	switch mode {
	case settings.FooterTokenBoth:
		// "131k | 0.0% | 262k" — used tokens | percentage | context window
		return fmt.Sprintf("%s | %.1f%% | %s", formatTokenCount(displayTokens), ctxFrac*100, windowLabel)
	case settings.FooterTokenCount:
		// "131k | 262k" — used tokens | context window
		return fmt.Sprintf("%s | %s", formatTokenCount(displayTokens), windowLabel)
	default:
		// "0.0% | 262k" — percentage | context window
		return fmt.Sprintf("%.1f%% | %s", ctxFrac*100, windowLabel)
	}
}
