package openai

// ProviderOptions holds OpenAI-specific per-call options.
type ProviderOptions struct {
	ExtraBody map[string]any `json:"extraBody,omitempty"`
	User      string         `json:"user,omitempty"`
}

// ReasoningEffort names supported reasoning effort levels.
type ReasoningEffort string

const (
	ReasoningEffortNone    ReasoningEffort = "none"
	ReasoningEffortMinimal ReasoningEffort = "minimal"
	ReasoningEffortLow     ReasoningEffort = "low"
	ReasoningEffortMedium  ReasoningEffort = "medium"
	ReasoningEffortHigh    ReasoningEffort = "high"
	ReasoningEffortXHigh   ReasoningEffort = "xhigh"
)
