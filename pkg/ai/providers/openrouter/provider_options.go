package openrouter

// ProviderOptions holds OpenRouter-specific per-call options.
type ProviderOptions struct {
	ReasoningEffort string         `json:"reasoningEffort,omitempty"`
	ExtraBody       map[string]any `json:"extraBody,omitempty"`
	Provider        map[string]any `json:"provider,omitempty"`
}
