package fuzzy

import "strings"

// Score returns a relevance score for query against target.
// Higher is better; -1 means no subsequence match.
func Score(query, target string) int {
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
