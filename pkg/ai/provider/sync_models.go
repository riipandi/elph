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
	"github.com/riipandi/elph/pkg/jsoncfg"
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
	Dir         string
	HTTPClient  *http.Client
	Data        ModelsDevData
	Reporter    ProviderProgressReporter
	DryRun      bool // compare only; populate Updated without writing files
	SkipLiveAPI bool // use models.dev catalog instead of live /models APIs
}

// PreviewModelsDevUpdates fetches models.dev and reports provider files that
// would change without writing or calling live provider APIs.
func PreviewModelsDevUpdates(opts UpdateModelsOptions) (UpdateModelsResult, error) {
	opts.DryRun = true
	opts.SkipLiveAPI = true
	return UpdateModelsFromModelsDev(opts)
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
		reportProviderProgress(opts.Reporter, ProviderProgressEvent{
			Phase:  ProviderProgressSync,
			Label:  "models.dev",
			Action: ProviderProgressFetchMeta,
		})
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

	syncTargets := listSyncProviderTargets(entries, data)
	total := len(syncTargets)

	result := UpdateModelsResult{Dir: dir}
	for i, target := range syncTargets {
		entry := target.entry
		providerID := target.providerID
		catalogID := target.catalogID
		catalogProvider := target.catalogProvider
		entryName := entry.Name()

		label := strings.TrimSpace(catalogProvider.Name)
		if label == "" {
			label = providerID
		}
		if target.skipNotInCatalog {
			result.Skipped = append(result.Skipped, entryName+": provider not in models.dev catalog")
			reportProviderProgress(opts.Reporter, ProviderProgressEvent{
				Phase:      ProviderProgressSync,
				ProviderID: providerID,
				Label:      label,
				Index:      i + 1,
				Total:      total,
				Action:     ProviderProgressSkipped,
				Detail:     "not in models.dev catalog",
			})
			continue
		}

		reportProviderProgress(opts.Reporter, ProviderProgressEvent{
			Phase:      ProviderProgressSync,
			ProviderID: providerID,
			Label:      label,
			Index:      i + 1,
			Total:      total,
			Action:     ProviderProgressWorking,
		})

		path := filepath.Join(dir, entryName)
		raw, err := os.ReadFile(path)
		if err != nil {
			return result, fmt.Errorf("provider %q: %w", providerID, err)
		}

		var cfg FileConfig
		if cfgErr := jsoncfg.Unmarshal(raw, &cfg); cfgErr != nil {
			return result, fmt.Errorf("provider %q: decode: %w", providerID, err)
		}

		var changed bool
		switch {
		case isLiveModelsProvider(providerID) && !opts.SkipLiveAPI:
			cfg, changed, err = syncLiveProviderModels(ctx, client, providerID, catalogID, cfg, data, catalogProvider, &result, entryName)
			if err != nil {
				return result, fmt.Errorf("provider %q: %w", providerID, err)
			}
		default:
			cfg, changed = syncCatalogProviderModels(providerID, catalogID, cfg, data, catalogProvider, &result, entryName)
		}

		cfg, backfillChanged := BackfillProviderThinking(providerID, cfg)
		changed = changed || backfillChanged

		if !changed {
			result.Skipped = append(result.Skipped, entryName+": already up to date")
			reportProviderProgress(opts.Reporter, ProviderProgressEvent{
				Phase:      ProviderProgressSync,
				ProviderID: providerID,
				Label:      label,
				Index:      i + 1,
				Total:      total,
				Action:     ProviderProgressUnchanged,
			})
			continue
		}

		if opts.DryRun {
			result.Updated = append(result.Updated, entryName)
			reportProviderProgress(opts.Reporter, ProviderProgressEvent{
				Phase:      ProviderProgressSync,
				ProviderID: providerID,
				Label:      label,
				Index:      i + 1,
				Total:      total,
				Action:     ProviderProgressSynced,
			})
			continue
		}

		if strings.TrimSpace(cfg.Name) == "" && strings.TrimSpace(catalogProvider.Name) != "" {
			cfg.Name = catalogProvider.Name
		}

		//nolint:gosec // intentionally writing API key to config file
		payload, err := json.MarshalIndent(cfg, "", "  ")
		if err != nil {
			return result, fmt.Errorf("provider %q: encode: %w", providerID, err)
		}
		payload = append(payload, '\n')
		if err := os.WriteFile(path, payload, 0o600); err != nil {
			return result, fmt.Errorf("provider %q: write: %w", providerID, err)
		}
		result.Updated = append(result.Updated, entryName)
		reportProviderProgress(opts.Reporter, ProviderProgressEvent{
			Phase:      ProviderProgressSync,
			ProviderID: providerID,
			Label:      label,
			Index:      i + 1,
			Total:      total,
			Action:     ProviderProgressSynced,
		})
	}

	return result, nil
}

type syncProviderTarget struct {
	entry            os.DirEntry
	providerID       string
	catalogID        string
	catalogProvider  ModelsDevProvider
	skipNotInCatalog bool
}

func listSyncProviderTargets(entries []os.DirEntry, data ModelsDevData) []syncProviderTarget {
	providerEntries, _ := jsoncfg.SelectProviderEntries(entries)
	targets := make([]syncProviderTarget, 0, len(providerEntries))
	for _, entry := range providerEntries {
		providerID, ok := jsoncfg.ProviderID(entry.Name())
		if !ok || providerID == "" {
			continue
		}
		catalogID := modelsDevProviderID(providerID)
		catalogProvider, inCatalog := data.Catalog.Providers[catalogID]
		if !isLiveModelsProvider(providerID) && !inCatalog {
			targets = append(targets, syncProviderTarget{
				entry:            entry,
				providerID:       providerID,
				catalogID:        catalogID,
				skipNotInCatalog: true,
			})
			continue
		}
		targets = append(targets, syncProviderTarget{
			entry:           entry,
			providerID:      providerID,
			catalogID:       catalogID,
			catalogProvider: catalogProvider,
		})
	}
	sort.Slice(targets, func(i, j int) bool {
		return targets[i].providerID < targets[j].providerID
	})
	return targets
}

func syncCatalogProviderModels(
	providerID string,
	catalogID string,
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
		src, ok := data.lookupModel(catalogID, model.ID)
		if !ok {
			result.Warnings = append(result.Warnings, fmt.Sprintf("%s: model %q not found in models.dev", entryName, model.ID))
			continue
		}
		fresh := modelConfigFromModelsDev(src, catalogProvider.NPM)
		updated := mergeModelConfigWithTemplate(providerID, model, fresh)
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
		if tmpl, ok := thinkingTemplateModel(providerID, modelID); ok {
			fresh = backfillModelThinking(fresh, tmpl)
		}
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

func syncLiveProviderModels(
	ctx context.Context,
	client *http.Client,
	providerID string,
	catalogID string,
	cfg FileConfig,
	data ModelsDevData,
	catalogProvider ModelsDevProvider,
	result *UpdateModelsResult,
	entryName string,
) (FileConfig, bool, error) {
	baseURL := strings.TrimSpace(cfg.BaseURL)
	if baseURL == "" {
		baseURL = defaultLiveModelsBaseURL(providerID)
	}

	apiKey, err := ResolveValueAllowMissingEnv(cfg.APIKey)
	if err != nil {
		return cfg, false, fmt.Errorf("resolve apiKey: %w", err)
	}
	headers, err := resolveHeadersAllowMissingEnv(cfg.Headers)
	if err != nil {
		return cfg, false, err
	}

	if liveModelsProviderRequiresAuth(providerID) && strings.TrimSpace(apiKey) == "" {
		result.Warnings = append(result.Warnings, fmt.Sprintf("%s: API key unavailable for live /models (%s); using models.dev catalog only", entryName, strings.TrimSpace(cfg.APIKey)))
		syncedCfg, changed := syncCatalogProviderModels(providerID, catalogID, cfg, data, catalogProvider, result, entryName)
		return syncedCfg, changed, nil
	}

	liveIDs, err := FetchLiveModels(ctx, client, LiveModelsOptions{
		BaseURL:    baseURL,
		APIKey:     apiKey,
		AuthHeader: cfg.AuthHeader,
		Headers:    headers,
	})
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

		if src, ok := data.lookupModel(catalogID, modelID); ok {
			fresh := modelConfigFromModelsDev(src, providerNPM)
			model = mergeModelConfigWithTemplate(providerID, model, fresh)
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
		result.Warnings = append(result.Warnings, fmt.Sprintf("%s: removed model %q not returned by live /models API", entryName, modelID))
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
