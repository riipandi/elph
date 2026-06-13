package provider

// API identifies the upstream protocol used to complete a turn.
type API string

const (
	APIOpenAICompletions API = "openai-completions"
	APIAnthropicMessages API = "anthropic-messages"
)

// Cost tracks per-million-token pricing for usage display.
type Cost struct {
	Input      float64 `json:"input"`
	Output     float64 `json:"output"`
	CacheRead  float64 `json:"cacheRead"`
	CacheWrite float64 `json:"cacheWrite"`
}

// ModelConfig describes a single model entry inside a provider file.
type ModelConfig struct {
	ID            string            `json:"id"`
	Name          string            `json:"name,omitempty"`
	API           API               `json:"api,omitempty"`
	BaseURL       string            `json:"baseUrl,omitempty"`
	Reasoning     bool              `json:"reasoning,omitempty"`
	Input         []string          `json:"input,omitempty"`
	ContextWindow int               `json:"contextWindow,omitempty"`
	MaxTokens     int               `json:"maxTokens,omitempty"`
	Temperature   *float64          `json:"temperature,omitempty"`
	Cost          *Cost             `json:"cost,omitempty"`
	Headers       map[string]string `json:"headers,omitempty"`
}

// FileConfig is the JSON schema for one provider file in ~/.elph/providers.
type FileConfig struct {
	Name       string            `json:"name,omitempty"`
	BaseURL    string            `json:"baseUrl,omitempty"`
	API        API               `json:"api,omitempty"`
	APIKey     string            `json:"apiKey,omitempty"`
	AuthHeader bool              `json:"authHeader,omitempty"`
	Headers    map[string]string `json:"headers,omitempty"`
	Models     []ModelConfig     `json:"models"`
}

// ResolvedModel is a normalized model entry ready for UI and API calls.
type ResolvedModel struct {
	ID            string
	Name          string
	ProviderID    string
	ProviderName  string
	API           API
	BaseURL       string
	Reasoning     bool
	Input         []string
	ContextWindow int
	MaxTokens     int
	Temperature   float64
	Cost          Cost
	Headers       map[string]string
}

// RegisteredProvider is a provider loaded from disk with normalized models.
type RegisteredProvider struct {
	ID     string
	Config FileConfig
	Models []ResolvedModel
}
