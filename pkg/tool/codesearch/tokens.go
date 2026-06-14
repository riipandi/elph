package codesearch

import (
	"os"
	"strings"
)

const (
	envGitHubPATToken   = "GITHUB_PERSONAL_ACCESS_TOKEN"
	envGitHubToken      = "GITHUB_TOKEN"
	envGHToken          = "GH_TOKEN"
	envGitHubPAT        = "GITHUB_PAT"
	envGitLabToken      = "GITLAB_TOKEN"
	envGitLabPrivate    = "GITLAB_PRIVATE_TOKEN"
	envPrivateToken     = "PRIVATE_TOKEN"
	envGitLabHost       = "GITLAB_HOST"
	envGitLabURL        = "GITLAB_URL"
	defaultGitLabAPIURL = "https://gitlab.com/api/v4"
)

// GitHubToken returns an optional GitHub token from the environment.
// Aligns with github-mcp-server (GITHUB_PERSONAL_ACCESS_TOKEN) plus common aliases.
func GitHubToken() string {
	for _, key := range []string{envGitHubPATToken, envGitHubToken, envGHToken, envGitHubPAT} {
		if v := strings.TrimSpace(os.Getenv(key)); v != "" {
			return v
		}
	}
	return ""
}

// GitLabToken returns the first configured GitLab token from the environment.
func GitLabToken() string {
	for _, key := range []string{envGitLabToken, envGitLabPrivate, envPrivateToken} {
		if v := strings.TrimSpace(os.Getenv(key)); v != "" {
			return v
		}
	}
	return ""
}

// GitLabAPIBase returns the GitLab API v4 base URL.
func GitLabAPIBase() string {
	raw := strings.TrimSpace(os.Getenv(envGitLabHost))
	if raw == "" {
		raw = strings.TrimSpace(os.Getenv(envGitLabURL))
	}
	if raw == "" {
		return defaultGitLabAPIURL
	}
	raw = strings.TrimRight(raw, "/")
	if strings.HasSuffix(raw, "/api/v4") {
		return raw
	}
	return raw + "/api/v4"
}
