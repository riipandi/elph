package command

import (
	"github.com/riipandi/elph/internal/fuzzy"
)

func commandScore(query string, cmd SlashCommand) int {
	best := -1
	for _, term := range commandTerms(cmd) {
		if score := fuzzy.Score(query, term); score > best {
			best = score
		}
	}
	return best
}

func commandTerms(cmd SlashCommand) []string {
	terms := make([]string, 0, 2+len(cmd.Aliases))
	terms = append(terms, cmd.Name)
	terms = append(terms, cmd.Aliases...)
	if cmd.Description != "" {
		terms = append(terms, cmd.Description)
	}
	return terms
}
