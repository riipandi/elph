package settings

import (
	"time"

	"github.com/riipandi/elph/pkg/ai/provider"
)

// RunModelsSync fetches models.dev and updates provider files, then records lastSync.
func RunModelsSync() (provider.UpdateModelsResult, error) {
	result, err := provider.UpdateModelsFromModelsDev(provider.UpdateModelsOptions{})
	if err != nil {
		return result, err
	}
	if err := MarkModelsSynced(time.Now()); err != nil {
		return result, err
	}
	return result, nil
}

// RunModelsSyncIfDue syncs only when the configured interval has elapsed.
func RunModelsSyncIfDue(now time.Time) (provider.UpdateModelsResult, bool, error) {
	cfg, err := Load()
	if err != nil {
		return provider.UpdateModelsResult{}, false, err
	}
	if !cfg.SyncDue(now) {
		return provider.UpdateModelsResult{}, false, nil
	}
	result, err := RunModelsSync()
	return result, true, err
}
