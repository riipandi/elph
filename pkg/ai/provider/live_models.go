package provider

import (
	"context"
	"fmt"
	"net/http"
	"strings"

	"github.com/riipandi/elph/pkg/ai/utils"
)

const (
	OpenCodeZenBaseURL = "https://opencode.ai/zen/v1"
	OpenCodeGoBaseURL  = "https://opencode.ai/zen/go/v1"
)

// CompatibleModelsResponse is the OpenAI-compatible /models payload.
type CompatibleModelsResponse struct {
	Object string                    `json:"object"`
	Data   []CompatibleModelEntry    `json:"data"`
}

// CompatibleModelEntry is one model entry from a /models endpoint.
type CompatibleModelEntry struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	OwnedBy string `json:"owned_by"`
}

// LiveModelsOptions configures a live /models fetch.
type LiveModelsOptions struct {
	BaseURL    string
	APIKey     string
	AuthHeader bool
	Headers    map[string]string
}

func isLiveModelsProvider(providerID string) bool {
	switch providerID {
	case "opencode", "opencode-go", "deepseek", "kimi":
		return true
	default:
		return false
	}
}

func liveModelsProviderRequiresAuth(providerID string) bool {
	switch providerID {
	case "deepseek", "kimi":
		return true
	default:
		return false
	}
}

func defaultLiveModelsBaseURL(providerID string) string {
	switch providerID {
	case "opencode-go":
		return OpenCodeGoBaseURL
	case "opencode":
		return OpenCodeZenBaseURL
	default:
		return ""
	}
}

func compatibleModelsURL(baseURL string) string {
	return strings.TrimSuffix(strings.TrimSpace(baseURL), "/") + "/models"
}

func liveModelsRequestHeaders(opts LiveModelsOptions) map[string]string {
	headers := make(map[string]string, len(opts.Headers)+1)
	for key, value := range opts.Headers {
		headers[key] = value
	}
	if opts.AuthHeader || (opts.APIKey != "" && headers["Authorization"] == "") {
		headers["Authorization"] = "Bearer " + opts.APIKey
	}
	return headers
}

// FetchLiveModels returns model IDs from an OpenAI-compatible /models endpoint.
func FetchLiveModels(ctx context.Context, client *http.Client, opts LiveModelsOptions) ([]string, error) {
	baseURL := strings.TrimSpace(opts.BaseURL)
	if baseURL == "" {
		return nil, fmt.Errorf("missing baseUrl")
	}

	var resp CompatibleModelsResponse
	if err := utils.GetJSONWithHeaders(ctx, client, compatibleModelsURL(baseURL), liveModelsRequestHeaders(opts), &resp); err != nil {
		return nil, err
	}

	ids := make([]string, 0, len(resp.Data))
	seen := make(map[string]struct{}, len(resp.Data))
	for _, entry := range resp.Data {
		id := strings.TrimSpace(entry.ID)
		if id == "" {
			continue
		}
		if _, ok := seen[id]; ok {
			continue
		}
		seen[id] = struct{}{}
		ids = append(ids, id)
	}
	if len(ids) == 0 {
		return nil, fmt.Errorf("no models returned from %s", compatibleModelsURL(baseURL))
	}
	return ids, nil
}