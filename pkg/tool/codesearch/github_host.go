package codesearch

import (
	"fmt"
	"net/url"
	"os"
	"strings"
)

const (
	envGitHubHost        = "GITHUB_HOST"
	defaultGitHubRESTURL = "https://api.github.com/"
)

// GitHubRESTBase returns the GitHub REST API root URL (with trailing slash).
// Follows github-mcp-server host rules: GITHUB_HOST for GHES / GHEC, default api.github.com.
func GitHubRESTBase() string {
	raw := strings.TrimSpace(os.Getenv(envGitHubHost))
	if raw == "" {
		return defaultGitHubRESTURL
	}
	u, err := url.Parse(raw)
	if err != nil || u.Scheme == "" || u.Hostname() == "" {
		return defaultGitHubRESTURL
	}

	hostname := strings.ToLower(u.Hostname())
	if hostname == "github.com" || strings.HasSuffix(hostname, ".github.com") {
		return defaultGitHubRESTURL
	}
	if hostname == "ghe.com" || strings.HasSuffix(hostname, ".ghe.com") {
		return fmt.Sprintf("https://api.%s/", hostname)
	}
	return fmt.Sprintf("%s://%s/api/v3/", u.Scheme, hostname)
}
