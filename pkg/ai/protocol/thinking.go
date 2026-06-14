package protocol

// ThinkingFormat selects how reasoning parameters are sent to OpenAI-compatible APIs.
type ThinkingFormat string

const (
	ThinkingFormatReasoningEffort ThinkingFormat = "reasoning_effort"
	ThinkingFormatOpenRouter      ThinkingFormat = "openrouter"
	ThinkingFormatQwen            ThinkingFormat = "qwen"
	ThinkingFormatDeepSeek        ThinkingFormat = "deepseek"
)

// ThinkingConfig is the resolved thinking payload for one turn.
type ThinkingConfig struct {
	Enabled bool

	BudgetTokens int

	Adaptive       bool
	AdaptiveEffort string

	ReasoningEffort string
	ThinkingFormat  ThinkingFormat
	EnableThinking  bool
}
