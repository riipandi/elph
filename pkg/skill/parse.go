package skill

import (
	"strings"

	"gopkg.in/yaml.v3"
)

type frontmatter struct {
	Name                    string `yaml:"name"`
	Description             string `yaml:"description"`
	ArgumentHint            string `yaml:"argument-hint"`
	Type                    string `yaml:"type"`
	DisableModelInvocation  bool   `yaml:"disableModelInvocation"`
	DisableModelInvocation2 bool   `yaml:"disable-model-invocation"`
}

func parseFrontmatter(raw string) (frontmatter, string, bool) {
	meta, body, ok := parseFrontmatterOnce(raw)
	if ok {
		return meta, body, true
	}
	if fixed := quoteYAMLDescriptionValues(raw); fixed != raw {
		return parseFrontmatterOnce(fixed)
	}
	return frontmatter{}, raw, false
}

func parseFrontmatterOnce(raw string) (frontmatter, string, bool) {
	raw = strings.TrimPrefix(raw, "\ufeff")
	trimmed := strings.TrimLeft(raw, " \t")
	if !strings.HasPrefix(trimmed, "---") {
		return frontmatter{}, raw, false
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
		return frontmatter{}, raw, false
	}

	meta := rest[:end]
	body := rest[end+4:]
	body = strings.TrimPrefix(body, "\r\n")
	body = strings.TrimPrefix(body, "\n")

	var parsed frontmatter
	if err := yaml.Unmarshal([]byte(meta), &parsed); err != nil {
		return frontmatter{}, raw, false
	}
	if parsed.DisableModelInvocation2 {
		parsed.DisableModelInvocation = true
	}
	return parsed, body, true
}

// quoteYAMLDescriptionValues wraps bare description values that contain colons so
// cross-client SKILL.md files with technically invalid YAML still parse.
func quoteYAMLDescriptionValues(raw string) string {
	raw = strings.TrimPrefix(raw, "\ufeff")
	trimmed := strings.TrimLeft(raw, " \t")
	if !strings.HasPrefix(trimmed, "---") {
		return raw
	}
	rest := trimmed[3:]
	if len(rest) > 0 && (rest[0] == '\n' || rest[0] == '\r') {
		if rest[0] == '\n' {
			rest = rest[1:]
		} else if len(rest) > 1 && rest[1] == '\n' {
			rest = rest[2:]
		} else {
			rest = rest[1:]
		}
	}
	end := strings.Index(rest, "\n---")
	if end < 0 {
		return raw
	}

	lines := strings.Split(rest[:end], "\n")
	changed := false
	for i, line := range lines {
		trimmedLine := strings.TrimSpace(line)
		if !strings.HasPrefix(trimmedLine, "description:") {
			continue
		}
		value := strings.TrimSpace(strings.TrimPrefix(trimmedLine, "description:"))
		if value == "" || value[0] == '"' || value[0] == '\'' || value == "|" || value == ">" {
			continue
		}
		if !strings.Contains(value, ":") {
			continue
		}
		escaped := strings.ReplaceAll(value, `"`, `\"`)
		lines[i] = `description: "` + escaped + `"`
		changed = true
	}
	if !changed {
		return raw
	}
	return "---\n" + strings.Join(lines, "\n") + rest[end:]
}

func firstNonEmptyLine(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		return trimmed
	}
	return ""
}

func normalizeType(raw string) string {
	return strings.ToLower(strings.TrimSpace(raw))
}
