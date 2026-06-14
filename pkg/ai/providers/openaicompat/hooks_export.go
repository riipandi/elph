package openaicompat

import elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"

// CompatHooks returns the default OpenAI-compatible hook set.
func CompatHooks() elphopenai.Hooks {
	return compatHooks()
}
