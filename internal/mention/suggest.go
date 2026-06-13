package mention

import (
	"sort"
	"strings"
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
	pathScore := fuzzyScore(query, entry.Path)
	baseScore := fuzzyScore(query, filepathBase(entry.Path))
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

// fuzzyScore returns a relevance score for query against target.
func fuzzyScore(query, target string) int {
	query = strings.ToLower(strings.TrimSpace(query))
	target = strings.ToLower(target)
	if query == "" {
		return 0
	}
	if target == "" {
		return -1
	}

	qi := 0
	score := 0
	prev := -2
	for ti := 0; ti < len(target) && qi < len(query); ti++ {
		if target[ti] == query[qi] {
			score++
			if ti == 0 {
				score += 8
			}
			if ti == prev+1 {
				score += 4
			}
			prev = ti
			qi++
		}
	}
	if qi != len(query) {
		return -1
	}
	return score
}
