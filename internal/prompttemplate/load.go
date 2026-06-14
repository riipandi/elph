package prompttemplate

import (
	"os"
	"path/filepath"
	"strings"
)

// Load discovers prompt templates from ~/.elph/prompts and <workDir>/.agents/elph/prompts.
// Project templates override global templates with the same name.
func Load(workDir string) []Template {
	byName := make(map[string]Template)
	order := make([]string, 0)

	if globalDir, err := GlobalDir(); err == nil {
		for _, t := range loadDir(globalDir, "global") {
			if _, exists := byName[t.Name]; !exists {
				order = append(order, t.Name)
			}
			byName[t.Name] = t
		}
	}

	for _, t := range loadDir(ProjectDir(workDir), "project") {
		if _, exists := byName[t.Name]; !exists {
			order = append(order, t.Name)
		}
		byName[t.Name] = t
	}

	out := make([]Template, 0, len(order))
	for _, name := range order {
		out = append(out, byName[name])
	}
	return out
}

func loadDir(dir, scope string) []Template {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}

	out := make([]Template, 0)
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".md") {
			continue
		}
		path := filepath.Join(dir, entry.Name())
		if t, ok := loadFile(path, scope); ok {
			out = append(out, t)
		}
	}
	return out
}

func loadFile(path, scope string) (Template, bool) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return Template{}, false
	}

	meta, body, hasMeta := ParseFrontmatter(string(raw))
	if !hasMeta {
		body = string(raw)
	}

	name := strings.TrimSuffix(filepath.Base(path), ".md")
	description := strings.TrimSpace(meta.Description)
	if description == "" {
		description = firstNonEmptyLine(body)
	}

	return Template{
		Name:         name,
		Description:  description,
		ArgumentHint: strings.TrimSpace(meta.ArgumentHint),
		Content:      body,
		FilePath:     path,
		Scope:        scope,
	}, true
}

func firstNonEmptyLine(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if len(trimmed) > 60 {
			return trimmed[:60] + "..."
		}
		return trimmed
	}
	return ""
}

// Expand returns substituted template content when input matches /name [args].
func Expand(input string, templates []Template) (expanded string, ok bool) {
	trimmed := strings.TrimLeft(input, " \t")
	if !strings.HasPrefix(trimmed, "/") {
		return "", false
	}

	body := strings.TrimSpace(strings.TrimPrefix(trimmed, "/"))
	if body == "" {
		return "", false
	}

	parts := strings.SplitN(body, " ", 2)
	name := parts[0]
	argsString := ""
	if len(parts) == 2 {
		argsString = parts[1]
	}

	for _, t := range templates {
		if strings.EqualFold(t.Name, name) {
			args := ParseArgs(argsString)
			return SubstituteArgs(t.Content, args), true
		}
	}
	return "", false
}
