package main

import (
	"fmt"
	"os"

	"github.com/riipandi/elph/internal/config"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/renderer"
	"github.com/spf13/cobra"
	"github.com/subosito/gotenv"
)

var (
	argVersionShort    bool
	argVersionSemantic bool
	rootEnvFile        string
)

var rootCmd = &cobra.Command{
	Use:   "elph",
	Short: "Minimalist AI agent companion",
	Long: `A terminal-native AI coding companion. Run without arguments to open the
interactive TUI—chat with your model, execute shell commands, switch agent
modes, and manage providers from one place.`,
	CompletionOptions: cobra.CompletionOptions{
		DisableDefaultCmd: true,
		HiddenDefaultCmd:  true,
	},
	PersistentPreRun: func(cmd *cobra.Command, args []string) {
		if rootEnvFile != "" {
			if err := gotenv.OverLoad(rootEnvFile); err != nil {
				fmt.Fprintf(os.Stderr, "failed to load env file %s: %v\n", rootEnvFile, err)
				os.Exit(1)
			}
		}
	},
	Run: func(cmd *cobra.Command, args []string) {
		if err := renderer.Render(); err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
	},
}

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Show the application version",
	Run: func(cmd *cobra.Command, args []string) {
		if argVersionShort {
			fmt.Printf("%s (%s)\n", config.AppVersion, config.BuildHash)
			return
		} else if argVersionSemantic {
			fmt.Printf("%s\n", config.AppVersion)
			return
		} else {
			fmt.Printf("%s %s (%s) %s\n", config.AppIdentifier, config.AppVersion, config.BuildHash, config.Platform)
		}
	},
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	os.Exit(0)
}

const rootHelpTemplate = `{{banner}}
{{with .Long}}{{. | trimTrailingWhitespaces}}

{{end}}Usage:
  {{.UseLine}}{{if .HasAvailableSubCommands}}
  {{.CommandPath}} [command]{{end}}{{if gt (len .Aliases) 0}}

Aliases:
  {{.NameAndAliases}}{{end}}{{if .HasExample}}

Examples:
{{.Example}}{{end}}{{if .HasAvailableSubCommands}}

Available Commands:{{range .Commands}}{{if (or .IsAvailableCommand (eq .Name "help"))}}
  {{rpad .Name .NamePadding }} {{.Short}}{{end}}{{end}}{{end}}{{if .HasAvailableLocalFlags}}

Flags:
{{.LocalFlags.FlagUsages | trimTrailingWhitespaces}}{{end}}{{if .HasAvailableInheritedFlags}}

Global Flags:
{{.InheritedFlags.FlagUsages | trimTrailingWhitespaces}}{{end}}{{if .HasHelpSubCommands}}

Additional help topics:{{range .Commands}}{{if .IsAdditionalHelpTopicCommand}}
  {{rpad .CommandPath .CommandPathPadding}} {{.Short}}{{end}}{{end}}{{end}}{{if .HasAvailableSubCommands}}

Use "{{.CommandPath}} [command] --help" for more information about a command.{{end}}
`

func helpBanner() string {
	info := []string{
		fmt.Sprintf("Welcome to %s v%s", config.AppName, config.AppVersion),
		fmt.Sprintf("Build %s (%s) %s", config.BuildHash, config.Platform, config.BuildDate),
	}
	return constants.JoinSideBySide(constants.LogoLines(), info, 2) + "\n"
}

func init() {
	cobra.AddTemplateFunc("banner", helpBanner)
	rootCmd.SetHelpTemplate(rootHelpTemplate)

	// Set `true` to disable the default help subcommand
	rootCmd.SetHelpCommand(&cobra.Command{Hidden: false})

	// Add version subcommand
	rootCmd.AddCommand(versionCmd)
	versionCmd.Flags().BoolVarP(&argVersionShort, "short", "s", false, "Show short version")
	versionCmd.Flags().BoolVarP(&argVersionSemantic, "semantic", "S", false, "Show semantic version")

	// Global flags
	rootCmd.PersistentFlags().StringVar(&rootEnvFile, "env-file", "", "Environment variable file (e.g., .env.local)")

	// Register subcommand
	rootCmd.AddCommand(doctorCmd)
	rootCmd.AddCommand(providerCmd)
}
