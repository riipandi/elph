package codesearch

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestGitHubRESTBaseDefault(t *testing.T) {
	clearCodeSearchEnv(t)
	require.Equal(t, "https://api.github.com/", GitHubRESTBase())
}

func TestGitHubRESTBaseGHEC(t *testing.T) {
	clearCodeSearchEnv(t)
	t.Setenv("GITHUB_HOST", "https://octocorp.ghe.com")
	require.Equal(t, "https://api.octocorp.ghe.com/", GitHubRESTBase())
}

func TestGitHubRESTBaseGHES(t *testing.T) {
	clearCodeSearchEnv(t)
	t.Setenv("GITHUB_HOST", "https://github.example.com")
	require.Equal(t, "https://github.example.com/api/v3/", GitHubRESTBase())
}
