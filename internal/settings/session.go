package settings

import (
	"strings"

	"github.com/riipandi/elph/internal/constants"
)

// SessionSettings stores per-user UI and runtime preferences.
type SessionSettings struct {
	ProviderID    string `json:"providerId,omitempty"`
	ModelID       string `json:"modelId,omitempty"`
	AgentMode     string `json:"agentMode,omitempty"`
	ThinkingLevel string `json:"thinkingLevel,omitempty"`
}

// AgentMode returns the persisted agent mode, defaulting to build.
func (s Settings) AgentMode() constants.AgentMode {
	return normalizeAgentMode(s.Session.AgentMode)
}

// ThinkingLevel returns the persisted thinking level, defaulting to high.
func (s Settings) ThinkingLevel() constants.ThinkingLevel {
	return normalizeThinkingLevel(s.Session.ThinkingLevel)
}

// ActiveProviderID returns the last selected provider id from settings.
func (s Settings) ActiveProviderID() string {
	return strings.TrimSpace(s.Session.ProviderID)
}

// ActiveModelID returns the last selected model id from settings.
func (s Settings) ActiveModelID() string {
	return strings.TrimSpace(s.Session.ModelID)
}

// Update loads settings, applies mutator, and saves the result.
func Update(mutator func(*Settings)) error {
	cfg, err := Load()
	if err != nil {
		return err
	}
	mutator(&cfg)
	return Save(cfg)
}

// SetActiveModel records the active provider/model selection.
func SetActiveModel(providerID, modelID string) error {
	return Update(func(cfg *Settings) {
		cfg.Session.ProviderID = strings.TrimSpace(providerID)
		cfg.Session.ModelID = strings.TrimSpace(modelID)
	})
}

// SetAgentMode records the active agent mode.
func SetAgentMode(mode constants.AgentMode) error {
	return Update(func(cfg *Settings) {
		cfg.Session.AgentMode = string(normalizeAgentMode(string(mode)))
	})
}

// SetThinkingLevel records the active thinking level.
func SetThinkingLevel(level constants.ThinkingLevel) error {
	return Update(func(cfg *Settings) {
		cfg.Session.ThinkingLevel = string(normalizeThinkingLevel(string(level)))
	})
}

func normalizeAgentMode(raw string) constants.AgentMode {
	mode := constants.AgentMode(strings.TrimSpace(raw))
	switch mode {
	case constants.ModeBuild, constants.ModePlan, constants.ModeAsk, constants.ModeBrave:
		return mode
	default:
		return constants.ModeBuild
	}
}

func normalizeThinkingLevel(raw string) constants.ThinkingLevel {
	level := constants.ThinkingLevel(strings.TrimSpace(raw))
	switch level {
	case constants.ThinkingOff,
		constants.ThinkingMinimal,
		constants.ThinkingLow,
		constants.ThinkingMedium,
		constants.ThinkingHigh,
		constants.ThinkingXHigh:
		return level
	default:
		return constants.ThinkingHigh
	}
}
