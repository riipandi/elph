package settings

// mergeSettings overlays non-empty project fields onto base (home/defaults).
// Pointer fields copy only when the overlay explicitly sets them.
func mergeSettings(base, overlay Settings) Settings {
	if overlay.Theme != "" {
		base.Theme = overlay.Theme
	}
	if overlay.ShowThinking != nil {
		base.ShowThinking = cloneBool(overlay.ShowThinking)
	}
	if overlay.AutoExpandThinking != nil {
		base.AutoExpandThinking = cloneBool(overlay.AutoExpandThinking)
	}
	if overlay.UseRawPaste != nil {
		base.UseRawPaste = cloneBool(overlay.UseRawPaste)
	}
	if overlay.StickyScroll != nil {
		base.StickyScroll = cloneBool(overlay.StickyScroll)
	}
	if overlay.PreferedResponseLanguage != "" {
		base.PreferedResponseLanguage = overlay.PreferedResponseLanguage
	}
	if overlay.SyncInterval != "" {
		base.SyncInterval = overlay.SyncInterval
	} else if overlay.Models != nil && overlay.Models.SyncInterval != "" {
		base.SyncInterval = overlay.Models.SyncInterval
	}
	base.Models = mergeModelsSettings(base.Models, overlay.Models)
	base.Session = mergeSessionSettings(base.Session, overlay.Session)
	if len(overlay.ThinkingBudgets) > 0 {
		if base.ThinkingBudgets == nil {
			base.ThinkingBudgets = make(map[string]int, len(overlay.ThinkingBudgets))
		}
		for k, v := range overlay.ThinkingBudgets {
			base.ThinkingBudgets[k] = v
		}
	}
	if overlay.MaxToolIterations != nil {
		val := *overlay.MaxToolIterations
		base.MaxToolIterations = &val
	}
	if overlay.AutoCompactContext != nil {
		base.AutoCompactContext = cloneBool(overlay.AutoCompactContext)
	}
	if overlay.AutoCompactLimit != nil {
		val := *overlay.AutoCompactLimit
		base.AutoCompactLimit = &val
	}
	base.Provider = mergeProviderSettings(base.Provider, overlay.Provider)
	return base
}

func mergeModelsSettings(base, overlay *ModelsSettings) *ModelsSettings {
	if overlay == nil {
		return base
	}
	if base == nil {
		base = &ModelsSettings{}
	}
	merged := *base
	if overlay.LastSync != "" {
		merged.LastSync = overlay.LastSync
	}
	if merged.LastSync == "" {
		return nil
	}
	return &merged
}

func mergeSessionSettings(base, overlay SessionSettings) SessionSettings {
	// Last-selected provider/model are persisted in home settings; project
	// settings only supply defaults when home has no selection.
	if base.ProviderID == "" && overlay.ProviderID != "" {
		base.ProviderID = overlay.ProviderID
	}
	if base.ModelID == "" && overlay.ModelID != "" {
		base.ModelID = overlay.ModelID
	}
	if overlay.AgentMode != "" {
		base.AgentMode = overlay.AgentMode
	}
	if overlay.ThinkingLevel != "" {
		base.ThinkingLevel = overlay.ThinkingLevel
	}
	return base
}

func cloneBool(v *bool) *bool {
	if v == nil {
		return nil
	}
	b := *v
	return &b
}
