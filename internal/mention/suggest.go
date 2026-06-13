package mention

import (
	"sort"
	"strings"

	"github.com/riipandi/elph/internal/fuzzy"
)

const maxSuggestions = 8

type scoredEntry struct {
	entry Entry
	score int
	idx   int
}

// Suggest returns entries that fuzzy-match query, best matches first.
func Suggest(query string, entries []Entry) []Entry {
	query = strings.ToLower(strings.TrimSpace(query))
	if len(entries) == 0 {
		return nil
	}
	if query == "" {
		limit := min(len(entries), maxSuggestions)
		out := make([]Entry, limit)
		for i := 0; i < limit; i++ {
			out[i] = entries[i]
		}
		return out
	}

	scored := make([]scoredEntry, 0, len(entries))
	for i, entry := range entries {
		if score := entryScore(query, entry); score >= 0 {
			scored = append(scored, scoredEntry{entry: entry, score: score, idx: i})
		}
	}

	sort.Slice(scored, func(i, j int) bool {
		if scored[i].score != scored[j].score {
			return scored[i].score > scored[j].score
		}
		return scored[i].idx < scored[j].idx
	})

	limit := min(len(scored), maxSuggestions)
	out := make([]Entry, limit)
	for i := 0; i < limit; i++ {
		out[i] = scored[i].entry
	}
	return out
}

func entryScore(query string, entry Entry) int {
	pathScore := fuzzy.Score(query, entry.Path)
	baseScore := fuzzy.Score(query, filepathBase(entry.Path))
	if baseScore > pathScore {
		return baseScore + 2
	}
	if entry.IsDir {
		return pathScore + 1
	}
	return pathScore
}

func filepathBase(path string) string {
	path = strings.TrimSuffix(path, "/")
	if i := strings.LastIndex(path, "/"); i >= 0 {
		return path[i+1:]
	}
	return path
}
