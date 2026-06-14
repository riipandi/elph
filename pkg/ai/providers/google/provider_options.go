package google

// ProviderOptions holds Google GenAI-specific per-call options.
type ProviderOptions struct {
	ThinkingBudget *int32 `json:"thinkingBudget,omitempty"`
}
