package openaicompat

// ProviderOptions holds OpenAI-compatible per-call options.
type ProviderOptions struct {
	ReasoningEffort string         `json:"reasoningEffort,omitempty"`
	ExtraBody       map[string]any `json:"extraBody,omitempty"`
	User            string         `json:"user,omitempty"`
}
