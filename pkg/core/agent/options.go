package agent

import "github.com/riipandi/elph/pkg/ai/provider"

// TurnOptions configures a single agent turn.
type TurnOptions struct {
	SystemPrompt string
	UserPrompt   string
	Model        string
	Provider     provider.Provider
}