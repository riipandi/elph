package provider

import (
	"os"

	"github.com/riipandi/elph/pkg/jsoncfg"
)

// EnsureStarterProviders writes built-in provider templates when the providers
// directory is missing or contains no provider config files.
// The second return value reports whether bootstrap ran.
func EnsureStarterProviders() (BootstrapResult, bool, error) {
	dir, err := ProvidersDir()
	if err != nil {
		return BootstrapResult{}, false, err
	}
	empty, err := providersDirEmpty(dir)
	if err != nil {
		return BootstrapResult{}, false, err
	}
	if !empty {
		return BootstrapResult{Dir: dir}, false, nil
	}
	result, err := BootstrapProviders(dir, false)
	if err != nil {
		return BootstrapResult{}, false, err
	}
	return result, len(result.Created) > 0 || len(result.Backfilled) > 0, nil
}

func providersDirEmpty(dir string) (bool, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return true, nil
		}
		return false, err
	}
	providerEntries, _ := jsoncfg.SelectProviderEntries(entries)
	return len(providerEntries) == 0, nil
}
