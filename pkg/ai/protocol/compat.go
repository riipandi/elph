package protocol

// Compat holds provider-specific API compatibility overrides (Pi-style).
type Compat struct {
	SupportsDeveloperRole    *bool  `json:"supportsDeveloperRole,omitempty"`
	SupportsReasoningEffort  *bool  `json:"supportsReasoningEffort,omitempty"`
	SupportsUsageInStreaming *bool  `json:"supportsUsageInStreaming,omitempty"`
	ForceAdaptiveThinking    bool   `json:"forceAdaptiveThinking,omitempty"`
	AllowEmptySignature      bool   `json:"allowEmptySignature,omitempty"`
	ThinkingFormat           string `json:"thinkingFormat,omitempty"`
	MaxTokensField           string `json:"maxTokensField,omitempty"`
}

func (c Compat) supportsDeveloperRole() bool {
	if c.SupportsDeveloperRole != nil {
		return *c.SupportsDeveloperRole
	}
	return false
}

func (c Compat) supportsReasoningEffort() bool {
	if c.SupportsReasoningEffort != nil {
		return *c.SupportsReasoningEffort
	}
	return true
}

func (c Compat) supportsUsageInStreaming() bool {
	if c.SupportsUsageInStreaming != nil {
		return *c.SupportsUsageInStreaming
	}
	return true
}

// DeveloperRoleSupported reports whether the upstream API accepts the developer role.
func (c Compat) DeveloperRoleSupported() bool { return c.supportsDeveloperRole() }

// ReasoningEffortSupported reports whether reasoning_effort is supported.
func (c Compat) ReasoningEffortSupported() bool { return c.supportsReasoningEffort() }

// UsageInStreamingSupported reports whether usage is included in stream chunks.
func (c Compat) UsageInStreamingSupported() bool { return c.supportsUsageInStreaming() }
