package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/spf13/cobra"
)

var providerForce bool

var providerCmd = &cobra.Command{
	Use:   "provider",
	Short: "Manage AI provider definitions",
}

var providerUpdateCmd = &cobra.Command{
	Use:   "update",
	Short: "Prefill primary provider templates in ~/.elph/providers",
	Long: strings.TrimSpace(`
Write starter provider files for OpenAI, Anthropic, OpenCode Zen, OpenCode Go,
DeepSeek, and Kimi.

Existing files are left untouched unless --force is passed.
Set API keys via environment variables referenced in the JSON files:
  OPENAI_API_KEY, ANTHROPIC_API_KEY, OPENCODE_API_KEY,
  DEEPSEEK_API_KEY, MOONSHOT_API_KEY
`),
	RunE: runProviderUpdate,
}

var providerUpdateModelsCmd = &cobra.Command{
	Use:   "models",
	Short: "Refresh model metadata from models.dev",
	Long: strings.TrimSpace(`
Fetch model metadata from https://models.dev/catalog.json and
https://models.dev/models.json, then update provider files under ~/.elph/providers.

OpenCode Zen, OpenCode Go, DeepSeek, and Kimi model lists come from live /models
endpoints (DeepSeek and Kimi require API keys); models.dev supplies context window,
pricing, and other metadata only.

Other providers use the models.dev catalog for both discovery and metadata.

Provider credentials, headers, and per-model overrides such as temperature are preserved.

Records the sync time in ~/.elph/settings.json. The TUI checks this timestamp
on startup and auto-syncs when models.syncInterval has elapsed (default 24h).
`),
	RunE: runProviderUpdateModels,
}

func runProviderUpdate(cmd *cobra.Command, args []string) error {
	result, err := provider.BootstrapProviders("", providerForce)
	if err != nil {
		return err
	}

	fmt.Printf("Providers directory: %s\n", result.Dir)
	if len(result.Created) > 0 {
		fmt.Printf("Created: %s\n", strings.Join(result.Created, ", "))
	}
	if len(result.Skipped) > 0 {
		fmt.Printf("Skipped (already exists): %s\n", strings.Join(result.Skipped, ", "))
	}
	if len(result.Created) == 0 && len(result.Skipped) > 0 {
		fmt.Fprintln(os.Stderr, "No files written. Use --force to overwrite existing provider files.")
	}
	return nil
}

func runProviderUpdateModels(cmd *cobra.Command, args []string) error {
	result, err := settings.RunModelsSync()
	if err != nil {
		return err
	}

	fmt.Printf("Providers directory: %s\n", result.Dir)
	if len(result.Updated) > 0 {
		fmt.Printf("Updated: %s\n", strings.Join(result.Updated, ", "))
	}
	if len(result.Skipped) > 0 {
		fmt.Printf("Skipped: %s\n", strings.Join(result.Skipped, ", "))
	}
	for _, warning := range result.Warnings {
		fmt.Fprintf(os.Stderr, "Warning: %s\n", warning)
	}
	if len(result.Updated) == 0 {
		fmt.Fprintln(os.Stderr, "No provider files were updated.")
	}
	return nil
}

func init() {
	providerUpdateCmd.Flags().BoolVar(&providerForce, "force", false, "Overwrite existing provider files")
	providerUpdateCmd.AddCommand(providerUpdateModelsCmd)
	providerCmd.AddCommand(providerUpdateCmd)
}
