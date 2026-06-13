package ai

import "github.com/riipandi/elph/pkg/ai/provider"

// LoadProviders reads all provider definitions from ~/.elph/providers.
func LoadProviders() (provider.Catalog, error) {
	return provider.LoadCatalog("")
}

// ResolveProvider loads user-defined providers and resolves the active one.
func ResolveProvider() provider.Config {
	return provider.Resolve()
}
