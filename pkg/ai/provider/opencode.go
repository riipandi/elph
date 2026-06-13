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

// OpenCodeModelsResponse is the OpenAI-compatible /models payload from OpenCode.
type OpenCodeModelsResponse struct {
	Object string               `json:"object"`
	Data   []OpenCodeModelEntry `json:"data"`
}

// OpenCodeModelEntry is one model entry from the OpenCode /models endpoint.
type OpenCodeModelEntry struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	OwnedBy string `json:"owned_by"`
}

func isOpenCodeProvider(providerID string) bool {
	switch providerID {
	case "opencode", "opencode-go":
		return true
	default:
		return false
	}
}

func defaultOpenCodeBaseURL(providerID string) string {
	switch providerID {
	case "opencode-go":
		return OpenCodeGoBaseURL
	default:
		return OpenCodeZenBaseURL
	}
}

func openCodeModelsURL(baseURL string) string {
	return strings.TrimSuffix(strings.TrimSpace(baseURL), "/") + "/models"
}

// FetchOpenCodeModels returns live model IDs from an OpenCode /models endpoint.
func FetchOpenCodeModels(ctx context.Context, client *http.Client, baseURL string) ([]string, error) {
	if strings.TrimSpace(baseURL) == "" {
		return nil, fmt.Errorf("missing baseUrl")
	}

	var resp OpenCodeModelsResponse
	if err := utils.GetJSON(ctx, client, openCodeModelsURL(baseURL), &resp); err != nil {
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
		return nil, fmt.Errorf("no models returned from %s", openCodeModelsURL(baseURL))
	}
	return ids, nil
}
