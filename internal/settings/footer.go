package settings

import "strings"

// FooterTokenDisplay defines how token usage is displayed in the footer.
type FooterTokenDisplay string

const (
	// FooterTokenPercentage shows percentage with context window: "0.0% | 262k"
	FooterTokenPercentage FooterTokenDisplay = "percentage"
	// FooterTokenBoth shows percentage and context window: "0.0% | 262k"
	FooterTokenBoth FooterTokenDisplay = "both"
	// FooterTokenCount shows used tokens with context window: "131k | 262k"
	FooterTokenCount FooterTokenDisplay = "count"
)

// ParseFooterTokenDisplay converts a string to FooterTokenDisplay, defaulting to "both".
func ParseFooterTokenDisplay(raw string) FooterTokenDisplay {
	mode := FooterTokenDisplay(strings.TrimSpace(strings.ToLower(raw)))
	switch mode {
	case FooterTokenPercentage, FooterTokenBoth, FooterTokenCount:
		return mode
	default:
		return FooterTokenBoth
	}
}

// SetFooterTokenDisplay persists the footer token display preference.
func SetFooterTokenDisplay(mode FooterTokenDisplay) error {
	return Update(func(cfg *Settings) {
		cfg.FooterTokenDisplay = string(mode)
	})
}
