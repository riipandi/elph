package command

import (
	"fmt"
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
)

// Context carries session state needed by slash command handlers.
type Context struct {
	WorkDir         string
	SystemPrompt    string
	LogPath         string
	RequestsLogPath string
	Catalog         provider.Catalog
	ProviderID      string
	ModelID         string
	ModelName       string

	pendingSwitch       *ModelSwitch
	pendingOpenSelector bool
	selectorCatalog     provider.Catalog
	selectorQuery       string
}

// ModelSwitch applies a new active provider/model to the session.
type ModelSwitch struct {
	Provider      provider.Provider
	ProviderID    string
	ProviderName  string
	ModelID       string
	ModelName     string
	ContextWindow int
	MaxTokens     int
	Catalog       provider.Catalog
}

// Result is the outcome of executing a slash command.
type Result struct {
	Output            string
	OK                bool
	Quit              bool
	Switch            *ModelSwitch
	OpenModelSelector bool
	SelectorCatalog   provider.Catalog
	SelectorQuery     string
}

// SlashCommand describes a built-in /command available in the TUI.
type SlashCommand struct {
	Name        string
	Aliases     []string
	Description string
	Args        []ArgChoice
	ArgsFunc    func(ctx Context) []ArgChoice
	Quits       bool
	Handler     func(ctx *Context, args string) string
}

// Execute runs a slash command from raw user input (e.g. "/help", "/model sonnet").
func Execute(input string, ctx Context) Result {
	name, args := parse(input)
	if name == "" {
		return Result{Output: "Usage: /help", OK: false}
	}

	for _, cmd := range builtin {
		if matches(cmd, name) {
			output := cmd.Handler(&ctx, args)
			return Result{
				Output:            output,
				OK:                true,
				Quit:              cmd.Quits,
				Switch:            ctx.pendingSwitch,
				OpenModelSelector: ctx.pendingOpenSelector,
				SelectorCatalog:   ctx.selectorCatalog,
				SelectorQuery:     ctx.selectorQuery,
			}
		}
	}

	return Result{
		Output: fmt.Sprintf("Unknown command: /%s\nType /help to see available commands.", name),
		OK:     false,
	}
}

// All returns built-in slash commands in catalog order.
func All() []SlashCommand {
	return append([]SlashCommand(nil), builtin...)
}

// Get returns a slash command by name or alias.
func Get(name string) (SlashCommand, bool) {
	name = strings.ToLower(strings.TrimSpace(name))
	for _, cmd := range builtin {
		if matches(cmd, name) {
			return cmd, true
		}
	}
	return SlashCommand{}, false
}

// HelpText returns a formatted list of slash commands.
func HelpText() string {
	return FormatHelp(builtin)
}

func init() {
	for i := range builtin {
		if builtin[i].Name == "help" {
			builtin[i].Handler = func(*Context, string) string { return FormatHelp(builtin) }
			return
		}
	}
}

func parse(input string) (name, args string) {
	trimmed := strings.TrimLeft(input, " \t")
	trimmed = strings.TrimPrefix(trimmed, "/")
	trimmed = strings.TrimSpace(trimmed)
	if trimmed == "" {
		return "", ""
	}

	parts := strings.SplitN(trimmed, " ", 2)
	name = strings.ToLower(parts[0])
	if len(parts) == 2 {
		args = strings.TrimSpace(parts[1])
	}
	return name, args
}

func matches(cmd SlashCommand, name string) bool {
	if strings.EqualFold(cmd.Name, name) {
		return true
	}
	for _, alias := range cmd.Aliases {
		if strings.EqualFold(alias, name) {
			return true
		}
	}
	return false
}

func notImplemented(name string) func(*Context, string) string {
	return func(*Context, string) string {
		return fmt.Sprintf("/%s: not yet implemented", name)
	}
}
