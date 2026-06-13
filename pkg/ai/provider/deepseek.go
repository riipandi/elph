package provider

import (
	"os"
)

const (
	deepSeekAPIKeyEnv  = "DEEPSEEK_API_KEY"
	deepSeekModelEnv   = "DEEPSEEK_MODEL"
	deepSeekBaseURL    = "https://api.deepseek.com"
	defaultDeepSeekModel = "deepseek-chat"
)

func newDeepSeekFromEnv() (*OpenAICompatible, error) {
	apiKey := os.Getenv(deepSeekAPIKeyEnv)
	if apiKey == "" {
		return nil, ErrMissingAPIKey
	}
	model := os.Getenv(deepSeekModelEnv)
	if model == "" {
		model = defaultDeepSeekModel
	}
	return NewOpenAICompatible(IDDeepSeek, apiKey, deepSeekBaseURL, model), nil
}