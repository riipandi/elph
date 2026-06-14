package skill

import (
	"context"
	"fmt"
)

// Invoke loads and formats an inline skill for the Skill tool.
func Invoke(ctx context.Context, workDir, name, args string) (string, error) {
	if err := Enter(ctx); err != nil {
		return "", err
	}

	def, err := Resolve(workDir, name)
	if err != nil {
		return "", err
	}
	if def.DisableModelInvocation {
		return "", fmt.Errorf("skill %q has disableModelInvocation enabled", def.Name)
	}
	typ := normalizeType(def.Type)
	if typ != "" && typ != TypeInline {
		return "", fmt.Errorf("skill %q has type %q; only inline skills can be invoked via Skill tool", def.Name, def.Type)
	}

	return FormatActivation(def, args), nil
}

// Format renders skill activation content (alias for FormatActivation).
func Format(def Definition, args string) string {
	return FormatActivation(def, args)
}