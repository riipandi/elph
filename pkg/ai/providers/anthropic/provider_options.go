package anthropic

// ProviderOptions holds Anthropic-specific per-call options.
type ProviderOptions struct {
	Effort                 string         `json:"effort,omitempty"`
	DisableParallelToolUse *bool          `json:"disableParallelToolUse,omitempty"`
	SendReasoning          *bool          `json:"sendReasoning,omitempty"`
	ExtraBody              map[string]any `json:"extraBody,omitempty"`
}

// Effort names adaptive thinking effort levels.
type Effort string

const (
	EffortLow    Effort = "low"
	EffortMedium Effort = "medium"
	EffortHigh   Effort = "high"
	EffortXHigh  Effort = "xhigh"
	EffortMax    Effort = "max"
)
