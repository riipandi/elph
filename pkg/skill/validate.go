package skill

import (
	"fmt"
	"strings"
	"unicode"
)

// ValidateName checks the agentskills.io name constraints.
func ValidateName(name string) error {
	if name == "" {
		return fmt.Errorf("skill name is required")
	}
	if len(name) > 64 {
		return fmt.Errorf("skill name exceeds 64 characters")
	}
	if name[0] == '-' || name[len(name)-1] == '-' {
		return fmt.Errorf("skill name cannot start or end with a hyphen")
	}
	if strings.Contains(name, "--") {
		return fmt.Errorf("skill name cannot contain consecutive hyphens")
	}
	for _, r := range name {
		if r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-' {
			continue
		}
		return fmt.Errorf("skill name %q contains invalid character %q", name, r)
	}
	return nil
}

// ValidateDescription checks the agentskills.io description constraints.
func ValidateDescription(description string) error {
	description = strings.TrimSpace(description)
	if description == "" {
		return fmt.Errorf("skill description is required")
	}
	if len(description) > 1024 {
		return fmt.Errorf("skill description exceeds 1024 characters")
	}
	return nil
}

// SanitizeName lowercases and strips invalid characters for lookup fallbacks.
func SanitizeName(name string) string {
	name = strings.ToLower(strings.TrimSpace(name))
	var b strings.Builder
	prevHyphen := false
	for _, r := range name {
		switch {
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9':
			b.WriteRune(r)
			prevHyphen = false
		case r == '-', r == '_', unicode.IsSpace(r):
			if !prevHyphen && b.Len() > 0 {
				b.WriteByte('-')
				prevHyphen = true
			}
		}
	}
	out := strings.Trim(b.String(), "-")
	if out == "" {
		return name
	}
	return out
}
