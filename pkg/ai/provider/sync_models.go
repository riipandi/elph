package provider

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/riipandi/elph/pkg/ai/utils"
)

// UpdateModelsResult reports provider files touched by a models.dev sync.
type UpdateModelsResult struct {
	Dir      string
	Updated  []string
	Skipped  []string
	Warnings []string
}

// UpdateModelsOptions configures a models.dev metadata sync.
type UpdateModelsOptions struct {
	Dir        string
	HTTPClient *http.Client
	Data       ModelsDevData
}

// UpdateModelsFromModelsDev refreshes model metadata in ~/.elph/providers
// using https://models.dev/catalog.json and https://models.dev/models.json.
func UpdateModelsFromModelsDev(opts UpdateModelsOptions) (UpdateModelsResult, error) {
	dir := opts.Dir
	if dir == "" {
		var err error
		dir, err = ProvidersDir()
		if err != nil {
			return UpdateModelsResult{}, err
		}
	}

	data := opts.Data
	if len(data.Catalog.Providers) == 0 && len(data.Models) == 0 {
		var err error
		data, err = FetchModelsDev(context.Background(), opts.HTTPClient)
		if err != nil {
			return UpdateModelsResult{}, err
		}
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return UpdateModelsResult{Dir: dir}, nil
		}
		return UpdateModelsResult{}, fmt.Errorf("read providers dir %q: %w", dir, err)
	}

	ctx := context.Background()
	client := opts.HTTPClient
	if client == nil {
		client = utils.NewHTTPClient()
	}

	result := UpdateModelsResult{Dir: dir}
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		providerID := strings.TrimSuffix(entry.Name(), ".json")
		if providerID == "" {
			continue
		}

		catalogProvider, inCatalog := data.Catalog.Providers[providerID]
		if !isOpenCodeProvider(providerID) && !inCatalog {
			result.Skipped = append(result.Skipped, entry.Name()+": provider not in models.dev catalog")
			continue
		}

		path := filepath.Join(dir, entry.Name())
		raw, err := os.ReadFile(path)
		if err != nil {
			return result, fmt.Errorf("provider %q: %w", providerID, err)
		}

		var cfg FileConfig
		if err := json.Unmarshal(raw, &cfg); err != nil {
			return result, fmt.Errorf("provider %q: decode: %w", providerID, err)
		}

		var changed bool
		if isOpenCodeProvider(providerID) {
			cfg, changed, err = syncOpenCodeProviderModels(ctx, client, providerID, cfg, data, catalogProvider, &result, entry.Name())
			if err != nil {
				return result, fmt.Errorf("provider %q: %w", providerID, err)
			}
		} else {
			cfg, changed = syncCatalogProviderModels(providerID, cfg, data, catalogProvider, &result, entry.Name())
		}

		if !changed {
			result.Skipped = append(result.Skipped, entry.Name()+": already up to date")
			continue
		}

		if strings.TrimSpace(cfg.Name) == "" && strings.TrimSpace(catalogProvider.Name) != "" {
			cfg.Name = catalogProvider.Name
		}

		payload, err := json.MarshalIndent(cfg, "", "  ")
		if err != nil {
			return result, fmt.Errorf("provider %q: encode: %w", providerID, err)
		}
		payload = append(payload, '\n')
		if err := os.WriteFile(path, payload, 0o644); err != nil {
			return result, fmt.Errorf("provider %q: write: %w", providerID, err)
		}
		result.Updated = append(result.Updated, entry.Name())
	}

	return result, nil
}

func syncCatalogProviderModels(
	providerID string,
	cfg FileConfig,
	data ModelsDevData,
	catalogProvider ModelsDevProvider,
	result *UpdateModelsResult,
	entryName string,
) (FileConfig, bool) {
	changed := false
	existing := make(map[string]struct{}, len(cfg.Models))
	for i, model := range cfg.Models {
		existing[model.ID] = struct{}{}
		src, ok := data.lookupModel(providerID, model.ID)
		if !ok {
			result.Warnings = append(result.Warnings, fmt.Sprintf("%s: model %q not found in models.dev", entryName, model.ID))
			continue
		}
		fresh := modelConfigFromModelsDev(src, catalogProvider.NPM)
		updated := mergeModelConfig(model, fresh)
		if modelConfigsEqual(model, updated) {
			continue
		}
		cfg.Models[i] = updated
		changed = true
	}

	var added []ModelConfig
	for modelID, src := range catalogProvider.Models {
		if _, ok := existing[modelID]; ok {
			continue
		}
		fresh := modelConfigFromModelsDev(src, catalogProvider.NPM)
		fresh.ID = modelID
		added = append(added, fresh)
	}
	if len(added) > 0 {
		sort.Slice(added, func(i, j int) bool {
			left := strings.ToLower(added[i].Name)
			if left == "" {
				left = strings.ToLower(added[i].ID)
			}
			right := strings.ToLower(added[j].Name)
			if right == "" {
				right = strings.ToLower(added[j].ID)
			}
			if left == right {
				return added[i].ID < added[j].ID
			}
			return left < right
		})
		cfg.Models = append(cfg.Models, added...)
		changed = true
	}
	return cfg, changed
}

func syncOpenCodeProviderModels(
	ctx context.Context,
	client *http.Client,
	providerID string,
	cfg FileConfig,
	data ModelsDevData,
	catalogProvider ModelsDevProvider,
	result *UpdateModelsResult,
	entryName string,
) (FileConfig, bool, error) {
	baseURL := strings.TrimSpace(cfg.BaseURL)
	if baseURL == "" {
		baseURL = defaultOpenCodeBaseURL(providerID)
	}

	liveIDs, err := FetchOpenCodeModels(ctx, client, baseURL)
	if err != nil {
		return cfg, false, err
	}

	existingByID := make(map[string]ModelConfig, len(cfg.Models))
	for _, model := range cfg.Models {
		if model.ID == "" {
			continue
		}
		existingByID[model.ID] = model
	}

	liveSet := make(map[string]struct{}, len(liveIDs))
	providerNPM := strings.TrimSpace(catalogProvider.NPM)
	updatedModels := make([]ModelConfig, 0, len(liveIDs))
	for _, modelID := range liveIDs {
		liveSet[modelID] = struct{}{}
		model := existingByID[modelID]
		if model.ID == "" {
			model = ModelConfig{ID: modelID}
		}

		if src, ok := data.lookupModel(providerID, modelID); ok {
			fresh := modelConfigFromModelsDev(src, providerNPM)
			model = mergeModelConfig(model, fresh)
		} else {
			result.Warnings = append(result.Warnings, fmt.Sprintf("%s: model %q not found in models.dev metadata", entryName, modelID))
			if strings.TrimSpace(model.Name) == "" {
				model.Name = modelID
			}
		}
		model.ID = modelID
		updatedModels = append(updatedModels, model)
	}

	for modelID := range existingByID {
		if _, ok := liveSet[modelID]; ok {
			continue
		}
		result.Warnings = append(result.Warnings, fmt.Sprintf("%s: removed model %q not returned by OpenCode API", entryName, modelID))
	}

	changed := !modelConfigListsEqual(cfg.Models, updatedModels)
	cfg.Models = updatedModels
	return cfg, changed, nil
}

func modelConfigListsEqual(a, b []ModelConfig) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if !modelConfigsEqual(a[i], b[i]) {
			return false
		}
	}
	return true
}

func modelConfigsEqual(a, b ModelConfig) bool {
	aa, err := json.Marshal(a)
	if err != nil {
		return false
	}
	bb, err := json.Marshal(b)
	if err != nil {
		return false
	}
	return string(aa) == string(bb)
}
