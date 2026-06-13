package provider

// modelsDevProviderID maps local provider filenames to models.dev catalog ids.
func modelsDevProviderID(providerID string) string {
	switch providerID {
	case "kimi":
		return "moonshotai"
	default:
		return providerID
	}
}
