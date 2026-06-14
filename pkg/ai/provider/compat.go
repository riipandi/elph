package provider

func mergeCompat(providerCompat, modelCompat Compat) Compat {
	out := providerCompat
	if modelCompat.SupportsDeveloperRole != nil {
		out.SupportsDeveloperRole = modelCompat.SupportsDeveloperRole
	}
	if modelCompat.SupportsReasoningEffort != nil {
		out.SupportsReasoningEffort = modelCompat.SupportsReasoningEffort
	}
	if modelCompat.SupportsUsageInStreaming != nil {
		out.SupportsUsageInStreaming = modelCompat.SupportsUsageInStreaming
	}
	if modelCompat.ForceAdaptiveThinking {
		out.ForceAdaptiveThinking = true
	}
	if modelCompat.AllowEmptySignature {
		out.AllowEmptySignature = true
	}
	if modelCompat.ThinkingFormat != "" {
		out.ThinkingFormat = modelCompat.ThinkingFormat
	}
	if modelCompat.MaxTokensField != "" {
		out.MaxTokensField = modelCompat.MaxTokensField
	}
	return out
}
