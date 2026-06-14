package provider

import (
	"encoding/json"
	"strconv"
	"strings"

	"github.com/riipandi/elph/internal/constants"
)

// ThinkingMapState describes a per-level entry in thinkingLevelMap.
type ThinkingMapState int

const (
	ThinkingMapDefault ThinkingMapState = iota
	ThinkingMapUnsupported
	ThinkingMapExplicit
)

// ThinkingMapValue is a parsed thinkingLevelMap entry.
type ThinkingMapValue struct {
	State ThinkingMapState
	Value string
}

var defaultThinkingBudgets = map[constants.ThinkingLevel]int{
	constants.ThinkingMinimal: 1024,
	constants.ThinkingLow:     4096,
	constants.ThinkingMedium:  10240,
	constants.ThinkingHigh:    32768,
	constants.ThinkingXHigh:   65536,
}

var thinkingLevelOrder = []constants.ThinkingLevel{
	constants.ThinkingOff,
	constants.ThinkingMinimal,
	constants.ThinkingLow,
	constants.ThinkingMedium,
	constants.ThinkingHigh,
	constants.ThinkingXHigh,
}

// ParseThinkingLevelMap decodes Pi-style tristate thinkingLevelMap values.
func ParseThinkingLevelMap(raw map[string]json.RawMessage) map[constants.ThinkingLevel]ThinkingMapValue {
	if len(raw) == 0 {
		return nil
	}
	out := make(map[constants.ThinkingLevel]ThinkingMapValue, len(raw))
	for key, value := range raw {
		level, ok := parseThinkingLevelKey(key)
		if !ok {
			continue
		}
		out[level] = parseThinkingLevelMapEntry(value)
	}
	return out
}

func parseThinkingLevelKey(key string) (constants.ThinkingLevel, bool) {
	switch strings.TrimSpace(key) {
	case string(constants.ThinkingOff):
		return constants.ThinkingOff, true
	case string(constants.ThinkingMinimal):
		return constants.ThinkingMinimal, true
	case string(constants.ThinkingLow):
		return constants.ThinkingLow, true
	case string(constants.ThinkingMedium):
		return constants.ThinkingMedium, true
	case string(constants.ThinkingHigh):
		return constants.ThinkingHigh, true
	case string(constants.ThinkingXHigh):
		return constants.ThinkingXHigh, true
	default:
		return "", false
	}
}

func parseThinkingLevelMapEntry(raw json.RawMessage) ThinkingMapValue {
	if len(raw) == 0 {
		return ThinkingMapValue{State: ThinkingMapDefault}
	}
	if string(raw) == "null" {
		return ThinkingMapValue{State: ThinkingMapUnsupported}
	}
	var value string
	if err := json.Unmarshal(raw, &value); err != nil {
		return ThinkingMapValue{State: ThinkingMapDefault}
	}
	return ThinkingMapValue{State: ThinkingMapExplicit, Value: strings.TrimSpace(value)}
}

// IsThinkingLevelSupported reports whether level is available for model.
func IsThinkingLevelSupported(level constants.ThinkingLevel, model ResolvedModel) bool {
	if !model.Reasoning && level != constants.ThinkingOff {
		return false
	}
	if entry, ok := model.ThinkingLevelMap[level]; ok && entry.State == ThinkingMapUnsupported {
		return false
	}
	return true
}

// ClampThinkingLevel returns the nearest supported thinking level for the model.
func ClampThinkingLevel(level constants.ThinkingLevel, model ResolvedModel) constants.ThinkingLevel {
	if IsThinkingLevelSupported(level, model) {
		return level
	}
	idx := thinkingLevelIndex(level)
	for i := idx + 1; i < len(thinkingLevelOrder); i++ {
		candidate := thinkingLevelOrder[i]
		if IsThinkingLevelSupported(candidate, model) {
			return candidate
		}
	}
	for i := idx - 1; i >= 0; i-- {
		candidate := thinkingLevelOrder[i]
		if IsThinkingLevelSupported(candidate, model) {
			return candidate
		}
	}
	return constants.ThinkingOff
}

// NextSupportedThinkingLevel cycles forward, skipping unsupported levels.
func NextSupportedThinkingLevel(current constants.ThinkingLevel, model ResolvedModel) constants.ThinkingLevel {
	next := current
	for range len(thinkingLevelOrder) {
		next = constants.NextThinkingLevel(next)
		if IsThinkingLevelSupported(next, model) {
			return next
		}
	}
	return current
}

// PrevSupportedThinkingLevel cycles backward, skipping unsupported levels.
func PrevSupportedThinkingLevel(current constants.ThinkingLevel, model ResolvedModel) constants.ThinkingLevel {
	next := current
	for range len(thinkingLevelOrder) {
		next = constants.PrevThinkingLevel(next)
		if IsThinkingLevelSupported(next, model) {
			return next
		}
	}
	return current
}

func thinkingLevelIndex(level constants.ThinkingLevel) int {
	for i, candidate := range thinkingLevelOrder {
		if candidate == level {
			return i
		}
	}
	return -1
}

// ResolveThinking maps the UI thinking level to provider request parameters.
func ResolveThinking(model ResolvedModel, level constants.ThinkingLevel, budgets map[string]int) ThinkingConfig {
	level = ClampThinkingLevel(level, model)
	if level == constants.ThinkingOff || !model.Reasoning {
		return ThinkingConfig{}
	}

	entry := mapEntry(model.ThinkingLevelMap, level)
	if entry.State == ThinkingMapUnsupported {
		return ThinkingConfig{}
	}

	format := thinkingFormatFor(model)
	adaptive := model.API == APIAnthropicMessages && model.Compat.ForceAdaptiveThinking

	cfg := ThinkingConfig{
		Enabled:        true,
		ThinkingFormat: format,
	}
	if adaptive {
		cfg.Adaptive = true
		cfg.AdaptiveEffort = resolveAdaptiveEffort(level, entry)
		return cfg
	}

	switch model.API {
	case APIAnthropicMessages:
		cfg.BudgetTokens = resolveBudgetTokens(level, entry, budgets)
	case APIOpenAICompletions:
		cfg = resolveOpenAIThinking(cfg, model, level, entry)
	default:
		cfg.Enabled = false
	}
	return cfg
}

func mapEntry(m map[constants.ThinkingLevel]ThinkingMapValue, level constants.ThinkingLevel) ThinkingMapValue {
	if m == nil {
		return ThinkingMapValue{State: ThinkingMapDefault}
	}
	entry, ok := m[level]
	if !ok {
		return ThinkingMapValue{State: ThinkingMapDefault}
	}
	return entry
}

func thinkingFormatFor(model ResolvedModel) ThinkingFormat {
	switch ThinkingFormat(strings.TrimSpace(model.Compat.ThinkingFormat)) {
	case ThinkingFormatOpenRouter:
		return ThinkingFormatOpenRouter
	case ThinkingFormatQwen:
		return ThinkingFormatQwen
	case ThinkingFormatDeepSeek:
		return ThinkingFormatDeepSeek
	default:
		return ThinkingFormatReasoningEffort
	}
}

func resolveAdaptiveEffort(level constants.ThinkingLevel, entry ThinkingMapValue) string {
	if entry.State == ThinkingMapExplicit && entry.Value != "" {
		return entry.Value
	}
	switch level {
	case constants.ThinkingMinimal, constants.ThinkingLow:
		return "low"
	case constants.ThinkingMedium:
		return "medium"
	case constants.ThinkingHigh:
		return "high"
	case constants.ThinkingXHigh:
		return "max"
	default:
		return "medium"
	}
}

func resolveBudgetTokens(level constants.ThinkingLevel, entry ThinkingMapValue, budgets map[string]int) int {
	if entry.State == ThinkingMapExplicit && entry.Value != "" {
		if tokens, err := strconv.Atoi(entry.Value); err == nil && tokens > 0 {
			return tokens
		}
	}
	if token, ok := budgets[string(level)]; ok && token > 0 {
		return token
	}
	if token, ok := defaultThinkingBudgets[level]; ok && token > 0 {
		return token
	}
	return defaultThinkingBudgets[constants.ThinkingHigh]
}

func resolveOpenAIThinking(cfg ThinkingConfig, model ResolvedModel, level constants.ThinkingLevel, entry ThinkingMapValue) ThinkingConfig {
	effort := resolveReasoningEffort(level, entry)
	switch cfg.ThinkingFormat {
	case ThinkingFormatQwen:
		cfg.EnableThinking = true
		return cfg
	case ThinkingFormatOpenRouter:
		if effort != "" {
			cfg.ReasoningEffort = effort
		}
		return cfg
	case ThinkingFormatDeepSeek:
		// DeepSeek reasoner models infer thinking from model id; no extra param required.
		return cfg
	default:
		if model.Compat.ReasoningEffortSupported() && effort != "" {
			cfg.ReasoningEffort = effort
			return cfg
		}
		cfg.EnableThinking = true
		return cfg
	}
}

func resolveReasoningEffort(level constants.ThinkingLevel, entry ThinkingMapValue) string {
	if entry.State == ThinkingMapExplicit && entry.Value != "" {
		return entry.Value
	}
	switch level {
	case constants.ThinkingMinimal:
		return "minimal"
	case constants.ThinkingLow:
		return "low"
	case constants.ThinkingMedium:
		return "medium"
	case constants.ThinkingHigh:
		return "high"
	case constants.ThinkingXHigh:
		return "high"
	default:
		return ""
	}
}
