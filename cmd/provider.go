package main

import (
	"fmt"
	"os"
	"strings"

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
Write starter provider files for OpenAI, Anthropic, and OpenCode Zen.

Existing files are left untouched unless --force is passed.
Set API keys via environment variables referenced in the JSON files:
  OPENAI_API_KEY, ANTHROPIC_API_KEY, OPENCODE_API_KEY
`),
	RunE: runProviderUpdate,
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

func init() {
	providerUpdateCmd.Flags().BoolVar(&providerForce, "force", false, "Overwrite existing provider files")
	providerCmd.AddCommand(providerUpdateCmd)
}
