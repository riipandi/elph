package command

import (
	"fmt"
	"strings"

	"github.com/riipandi/elph/internal/prompt/template"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tools/goal"
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

	ContextUsage bool // set by /context handler

	// ClearSystemPrompt signals the renderer to clear the session system prompt
	// before starting the agent turn, saving tokens and keeping the model focused.
	ClearSystemPrompt bool

	// SystemPromptOverride, when non-empty, replaces the session system prompt
	// for the upcoming agent turn.
	SystemPromptOverride string

	// GoalManager is an interface for the session goal manager.
	GoalManager *goal.Manager

	CompactHistory bool // set by /compact handler
	CompactRatio   int  // compaction target percentage

	pendingSwitch         *ModelSwitch
	pendingOpenSelector   bool
	pendingAgentPrompt    string // agent prompt to use for next turn
	selectorCatalog       provider.Catalog
	selectorQuery         string
	pendingDetailLabel    string
	pendingDetailBody     string
	pendingDetailExpanded bool

	PromptTemplates []template.Template
	Skills          []SlashSkill
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
	Input         []string
	Cost          provider.Cost
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
	AgentPrompt       string
	DetailLabel       string
	DetailBody        string
	DetailExpanded    bool
	ContextUsage      bool   // set by /context handler
	CompactHistory    bool   // signal to hard-compact conversation history
	CompactRatio      int    // compaction target percentage (0 = use default)
	ClearSystemPrompt bool   // clear session system prompt for this turn
	SystemPrompt      string // override session system prompt for this turn
}

// SlashCommand describes a built-in /command available in the TUI.
type SlashCommand struct {
	Name         string
	Aliases      []string
	Description  string
	ArgumentHint string
	Args         []ArgChoice
	ArgsFunc     func(ctx Context) []ArgChoice
	Quits        bool
	Prompt       bool
	Skill        bool
	Handler      func(ctx *Context, args string) string
}

// Execute runs a slash command from raw user input (e.g. "/help", "/model sonnet").
func Execute(input string, ctx Context) Result {
	name, args := parse(input)
	if name == "" {
		return Result{Output: "Usage: /help", OK: false}
	}
	if strings.EqualFold(name, "help") {
		return Result{Output: FormatHelp(allCommands(ctx)), OK: true}
	}

	for _, cmd := range allCommands(ctx) {
		if !matches(cmd, name) {
			continue
		}
		if cmd.Prompt {
			expanded, ok := template.Expand(input, ctx.PromptTemplates)
			if !ok || strings.TrimSpace(expanded) == "" {
				return Result{
					Output: fmt.Sprintf("/%s: prompt template is empty", name),
					OK:     false,
				}
			}
			return Result{OK: true, AgentPrompt: expanded}
		}
		if cmd.Skill {
			expanded, err := expandSkillSlash(ctx, cmd, args)
			if err != nil {
				return Result{
					Output: fmt.Sprintf("/%s: %v", cmd.Name, err),
					OK:     false,
				}
			}
			return Result{
				OK:          true,
				AgentPrompt: expanded.AgentPrompt,
				DetailLabel: expanded.DetailLabel,
				DetailBody:  expanded.DetailBody,
			}
		}
		if cmd.Handler == nil {
			return Result{
				Output: fmt.Sprintf("/%s: not yet implemented", name),
				OK:     true,
			}
		}
		output := cmd.Handler(&ctx, args)
		return Result{
			Output:            output,
			OK:                true,
			Quit:              cmd.Quits,
			Switch:            ctx.pendingSwitch,
			OpenModelSelector: ctx.pendingOpenSelector,
			AgentPrompt:       ctx.pendingAgentPrompt,
			SelectorCatalog:   ctx.selectorCatalog,
			SelectorQuery:     ctx.selectorQuery,
			DetailLabel:       ctx.pendingDetailLabel,
			DetailBody:        ctx.pendingDetailBody,
			DetailExpanded:    ctx.pendingDetailExpanded,
			ContextUsage:      ctx.ContextUsage,
			CompactHistory:    ctx.CompactHistory,
			CompactRatio:      ctx.CompactRatio,
			ClearSystemPrompt: ctx.ClearSystemPrompt,
			SystemPrompt:      ctx.SystemPromptOverride,
		}
	}

	return Result{
		Output: fmt.Sprintf("Unknown command: /%s\nType /help to see available commands.", name),
		OK:     false,
	}
}

// All returns built-in and prompt-template slash commands.
func All(ctx Context) []SlashCommand {
	return allCommands(ctx)
}

// Get returns a slash command by name or alias.
func Get(name string, ctx Context) (SlashCommand, bool) {
	name = strings.ToLower(strings.TrimSpace(name))
	for _, cmd := range allCommands(ctx) {
		if matches(cmd, name) {
			return cmd, true
		}
	}
	return SlashCommand{}, false
}

// HelpText returns a formatted list of slash commands.
func HelpText(ctx Context) string {
	return FormatHelp(allCommands(ctx))
}

func allCommands(ctx Context) []SlashCommand {
	out := append([]SlashCommand(nil), builtin...)
	seen := builtinNames()
	for _, t := range ctx.PromptTemplates {
		key := strings.ToLower(t.Name)
		if seen[key] {
			continue
		}
		out = append(out, templateCommand(t))
		seen[key] = true
	}
	for _, cmd := range skillCommands(ctx.Skills) {
		key := strings.ToLower(cmd.Name)
		if seen[key] {
			continue
		}
		out = append(out, cmd)
		seen[key] = true
	}
	return out
}

func builtinNames() map[string]bool {
	seen := make(map[string]bool, len(builtin))
	for _, cmd := range builtin {
		seen[strings.ToLower(cmd.Name)] = true
		for _, alias := range cmd.Aliases {
			seen[strings.ToLower(alias)] = true
		}
	}
	return seen
}

func templateCommand(t template.Template) SlashCommand {
	return SlashCommand{
		Name:         t.Name,
		Description:  t.Description,
		ArgumentHint: t.ArgumentHint,
		Prompt:       true,
		Handler:      func(*Context, string) string { return "" },
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
