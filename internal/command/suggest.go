package command

import (
	"sort"
	"strings"
)

const maxSuggestions = 8

type scoredCommand struct {
	cmd   SlashCommand
	score int
	idx   int
}

// Suggest returns slash commands that fuzzy-match query, best matches first.
// An empty query returns the first maxSuggestions commands in catalog order.
func Suggest(query string, ctx Context) []SlashCommand {
	commands := allCommands(ctx)
	query = normalizeSuggestQuery(query)
	if query == "" {
		out := make([]SlashCommand, 0, maxSuggestions)
		for _, cmd := range commands {
			out = append(out, cmd)
			if len(out) >= maxSuggestions {
				break
			}
		}
		return out
	}

	scored := make([]scoredCommand, 0, len(commands))
	for i, cmd := range commands {
		if score := commandScore(query, cmd); score >= 0 {
			scored = append(scored, scoredCommand{cmd: cmd, score: score, idx: i})
		}
	}

	sort.Slice(scored, func(i, j int) bool {
		if scored[i].score != scored[j].score {
			return scored[i].score > scored[j].score
		}
		return scored[i].idx < scored[j].idx
	})

	limit := min(len(scored), maxSuggestions)
	out := make([]SlashCommand, limit)
	for i := 0; i < limit; i++ {
		out[i] = scored[i].cmd
	}
	return out
}

// CompleteInput returns the full slash command input for the selected suggestion.
func CompleteInput(selected SlashCommand, ctx Context) string {
	input := "/" + selected.Name
	if len(EffectiveArgs(selected, ctx)) > 0 {
		input += " "
	}
	return input
}

func normalizeSuggestQuery(query string) string {
	query = strings.ToLower(strings.TrimSpace(query))
	if idx := strings.Index(query, " "); idx >= 0 {
		query = query[:idx]
	}
	return query
}
