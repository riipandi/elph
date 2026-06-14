// Package skill implements the Agent Skills format (https://agentskills.io/specification)
// for discovery and invocation in Elph.
package skill

const (
	// FileName is the required skill instruction file.
	FileName = "SKILL.md"

	// TypeInline is the only skill type invokable via the Skill tool.
	TypeInline = "inline"

	// MaxNestingDepth is the maximum Skill tool nesting depth per turn.
	MaxNestingDepth = 3
)

// Definition is a loaded SKILL.md entry.
type Definition struct {
	Name                   string
	Description            string
	ArgumentHint           string // optional slash-input hint (prompt-template frontmatter)
	Location               string // absolute path to SKILL.md
	BaseDir                string // skill root directory
	Type                   string
	DisableModelInvocation bool
	Body                   string // markdown instructions after frontmatter
}

// Invokable reports whether the Skill tool may load this skill.
func (d Definition) Invokable() bool {
	if d.DisableModelInvocation {
		return false
	}
	typ := normalizeType(d.Type)
	return typ == "" || typ == TypeInline
}
