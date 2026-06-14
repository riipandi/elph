package prompttemplate

import (
	"strings"

	"gopkg.in/yaml.v3"
)

type frontmatterFields struct {
	Description  string `yaml:"description"`
	ArgumentHint string `yaml:"argument-hint"`
}

// ParseFrontmatter splits optional YAML frontmatter from markdown body.
func ParseFrontmatter(raw string) (fields frontmatterFields, body string, ok bool) {
	raw = strings.TrimPrefix(raw, "\ufeff")
	trimmed := strings.TrimLeft(raw, " \t")
	if !strings.HasPrefix(trimmed, "---") {
		return frontmatterFields{}, raw, false
	}

	rest := trimmed[3:]
	if len(rest) > 0 && rest[0] == '\n' {
		rest = rest[1:]
	} else if len(rest) > 0 && rest[0] == '\r' {
		if len(rest) > 1 && rest[1] == '\n' {
			rest = rest[2:]
		} else {
			rest = rest[1:]
		}
	}

	end := strings.Index(rest, "\n---")
	if end < 0 {
		return frontmatterFields{}, raw, false
	}

	meta := rest[:end]
	body = rest[end+4:]
	body = strings.TrimPrefix(body, "\r\n")
	body = strings.TrimPrefix(body, "\n")

	var parsed frontmatterFields
	if err := yaml.Unmarshal([]byte(meta), &parsed); err != nil {
		return frontmatterFields{}, raw, false
	}
	return parsed, body, true
}