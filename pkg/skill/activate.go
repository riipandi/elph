package skill

import (
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"
)

const maxListedResources = 50

var resourceSubdirs = []string{"scripts", "references", "assets"}

// FormatActivation renders tier-2 skill instructions for the model (Skill tool and /skill:<name>).
func FormatActivation(def Definition, args string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "<skill_content name=%q>\n", xmlAttr(def.Name))
	b.WriteString("Apply this skill's workflow internally. User-visible output must follow system prompt Output rules — direct answers only; no skill names, mode labels, or meta footers unless the user explicitly requested that phrasing.\n\n")
	writeSkillBody(&b, def)
	fmt.Fprintf(&b, "\n\nSkill directory: %s\n", def.BaseDir)
	b.WriteString("Relative paths in this skill are relative to the skill directory.\n")

	if files, truncated := listResources(def.BaseDir); len(files) > 0 {
		b.WriteString("\n<skill_resources>\n")
		for _, rel := range files {
			fmt.Fprintf(&b, "  <file>%s</file>\n", xmlText(rel))
		}
		if truncated {
			b.WriteString("  <!-- listing may be incomplete -->\n")
		}
		b.WriteString("</skill_resources>\n")
	}

	if args = strings.TrimSpace(args); args != "" {
		b.WriteString("\n<user_args>\n")
		b.WriteString(args)
		b.WriteString("\n</user_args>\n")
	}
	b.WriteString("</skill_content>")
	return b.String()
}

// IsActivationContent reports whether s is structured skill activation output.
func IsActivationContent(s string) bool {
	return strings.Contains(s, "<skill_content")
}

func listResources(baseDir string) ([]string, bool) {
	out := make([]string, 0, 8)
	truncated := false

	for _, sub := range resourceSubdirs {
		root := filepath.Join(baseDir, sub)
		_ = filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if d.IsDir() {
				name := d.Name()
				if strings.HasPrefix(name, ".") {
					return filepath.SkipDir
				}
				return nil
			}
			rel, relErr := filepath.Rel(baseDir, path)
			if relErr != nil {
				return nil
			}
			out = append(out, filepath.ToSlash(rel))
			if len(out) >= maxListedResources {
				truncated = true
				return fs.SkipAll
			}
			return nil
		})
		if truncated {
			break
		}
	}
	return out, truncated
}

func xmlAttr(s string) string {
	return xmlText(s)
}

func xmlText(s string) string {
	s = strings.ReplaceAll(s, "&", "&amp;")
	s = strings.ReplaceAll(s, "<", "&lt;")
	s = strings.ReplaceAll(s, ">", "&gt;")
	s = strings.ReplaceAll(s, `"`, "&quot;")
	return s
}