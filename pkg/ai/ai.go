package ai

import "github.com/riipandi/elph/pkg/ai/provider"

// ResolveProvider loads the default upstream provider from environment variables.
func ResolveProvider() provider.Config {
	return provider.ResolveFromEnv()
}