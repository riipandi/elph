package skill

import "fmt"

// PromptContent loads a skill and formats it as an agent user prompt.
// Used by /skill:<name> slash commands (no nesting-depth accounting).
func PromptContent(workDir, name, args string) (string, error) {
	def, err := Resolve(workDir, name)
	if err != nil {
		return "", err
	}
	content := FormatActivation(def, args)
	if content == "" {
		return "", fmt.Errorf("skill %q is empty", name)
	}
	return content, nil
}
