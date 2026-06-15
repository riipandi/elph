package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/jsoncfg"
)

func printConnectResult(result provider.BootstrapResult) {
	fmt.Printf("Setting up providers in %s\n\n", result.Dir)
	printBootstrapChanges(result)

	if len(result.Created) == 0 && len(result.Backfilled) == 0 {
		if len(result.Skipped) > 0 {
			fmt.Println("All provider files are already set up.")
			fmt.Println("Use --force to replace existing files with built-in templates.")
		} else {
			fmt.Println("No provider files were written.")
		}
		fmt.Println("\nNext: set API keys, then run `elph provider update` to sync model catalogs.")
		return
	}

	fmt.Print("\n")
	printBootstrapSummary(result)
	fmt.Println("\nNext: set API keys, then run `elph provider update` to sync model catalogs.")
}

func printProviderUpdateResult(bootstrap provider.BootstrapResult, sync provider.UpdateModelsResult) {
	fmt.Printf("Updating providers in %s\n\n", bootstrap.Dir)

	fmt.Println("[1/2] Provider files")
	printBootstrapChanges(bootstrap)
	if bootstrapQuiet(bootstrap) {
		fmt.Println("  · All provider files are up to date")
	}

	fmt.Println("\n[2/2] Model catalogs")
	printModelsSyncChanges(sync)
	if syncQuiet(sync) {
		fmt.Println("  · All model catalogs are up to date")
	}

	fmt.Println()
	printUpdateSummary(bootstrap, sync)
}

func printBootstrapChanges(result provider.BootstrapResult) {
	if len(result.Created) > 0 {
		fmt.Printf("  + Created %s\n", joinProviderNames(result.Created))
	}
	if len(result.Backfilled) > 0 {
		fmt.Printf("  ~ Added thinking metadata to %s\n", joinProviderNames(result.Backfilled))
	}
	if len(result.Skipped) > 0 {
		fmt.Printf("  · Unchanged %s\n", joinProviderNames(result.Skipped))
	}
}

func printModelsSyncChanges(result provider.UpdateModelsResult) {
	if len(result.Updated) > 0 {
		fmt.Printf("  + Synced %s\n", joinProviderNames(result.Updated))
	}
	for _, entry := range result.Skipped {
		file, reason := splitProviderLogEntry(entry)
		switch reason {
		case "already up to date":
			fmt.Printf("  · Up to date %s\n", providerLabel(file))
		case "provider not in models.dev catalog":
			fmt.Printf("  · Skipped %s (not in models.dev catalog)\n", providerLabel(file))
		default:
			if reason != "" {
				fmt.Printf("  · Skipped %s (%s)\n", providerLabel(file), reason)
			} else {
				fmt.Printf("  · Skipped %s\n", providerLabel(file))
			}
		}
	}
	for _, warning := range result.Warnings {
		fmt.Fprintf(os.Stderr, "  ! %s\n", humanizeSyncWarning(warning))
	}
}

func printBootstrapSummary(result provider.BootstrapResult) {
	parts := make([]string, 0, 2)
	if n := len(result.Created); n > 0 {
		parts = append(parts, fmt.Sprintf("%d created", n))
	}
	if n := len(result.Backfilled); n > 0 {
		parts = append(parts, fmt.Sprintf("%d updated with thinking metadata", n))
	}
	if len(parts) == 0 {
		return
	}
	fmt.Printf("Done. Provider files: %s.\n", strings.Join(parts, ", "))
}

func printUpdateSummary(bootstrap provider.BootstrapResult, sync provider.UpdateModelsResult) {
	changed := len(bootstrap.Created) + len(bootstrap.Backfilled) + len(sync.Updated)
	unchanged := len(bootstrap.Skipped) + countUpToDateSkipped(sync.Skipped)

	switch {
	case changed == 0 && unchanged == 0 && len(sync.Warnings) == 0:
		fmt.Println("Nothing to update.")
	case changed == 0 && unchanged > 0:
		fmt.Printf("Done. Everything is up to date (%d provider%s unchanged).\n", unchanged, plural(unchanged))
	default:
		parts := make([]string, 0, 3)
		if n := len(sync.Updated); n > 0 {
			parts = append(parts, fmt.Sprintf("%d catalog%s synced", n, plural(n)))
		}
		if n := len(bootstrap.Created) + len(bootstrap.Backfilled); n > 0 {
			parts = append(parts, fmt.Sprintf("%d provider file%s updated", n, plural(n)))
		}
		if unchanged > 0 {
			parts = append(parts, fmt.Sprintf("%d unchanged", unchanged))
		}
		fmt.Printf("Done. %s.\n", strings.Join(parts, ", "))
	}

	if len(sync.Warnings) > 0 {
		fmt.Fprintf(os.Stderr, "%d warning%s — see notes above.\n", len(sync.Warnings), plural(len(sync.Warnings)))
	}
}

func bootstrapQuiet(result provider.BootstrapResult) bool {
	return len(result.Created) == 0 && len(result.Backfilled) == 0
}

func syncQuiet(result provider.UpdateModelsResult) bool {
	return len(result.Updated) == 0 && len(result.Warnings) == 0
}

func countUpToDateSkipped(skipped []string) int {
	n := 0
	for _, entry := range skipped {
		_, reason := splitProviderLogEntry(entry)
		if reason == "already up to date" {
			n++
		}
	}
	return n
}

func splitProviderLogEntry(entry string) (file, reason string) {
	entry = strings.TrimSpace(entry)
	if entry == "" {
		return "", ""
	}
	file, reason, ok := strings.Cut(entry, ":")
	if !ok {
		return entry, ""
	}
	return strings.TrimSpace(file), strings.TrimSpace(reason)
}

func joinProviderNames(files []string) string {
	labels := make([]string, 0, len(files))
	for _, file := range files {
		labels = append(labels, providerLabel(file))
	}
	return strings.Join(labels, ", ")
}

func providerLabel(file string) string {
	file = strings.TrimSpace(file)
	if file == "" {
		return file
	}
	if id, ok := jsoncfg.ProviderID(file); ok {
		return id
	}
	return strings.TrimSuffix(strings.TrimSuffix(file, jsoncfg.ExtJSONC), jsoncfg.ExtJSON)
}

func humanizeSyncWarning(warning string) string {
	warning = strings.TrimSpace(warning)
	file, msg := splitProviderLogEntry(warning)
	label := providerLabel(file)

	switch {
	case strings.Contains(msg, "API key unavailable for live /models"):
		if label != "" {
			return fmt.Sprintf("%s: no API key — synced from models.dev only (live /models unavailable)", label)
		}
		return "No API key — synced from models.dev only (live /models unavailable)"
	case strings.Contains(msg, "not found in models.dev"):
		if label != "" {
			return fmt.Sprintf("%s: %s", label, msg)
		}
		return msg
	case strings.Contains(msg, "not returned by live /models API"):
		if label != "" {
			return fmt.Sprintf("%s: %s", label, msg)
		}
		return msg
	default:
		if label != "" && msg != "" {
			return fmt.Sprintf("%s: %s", label, msg)
		}
		return warning
	}
}

func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}
