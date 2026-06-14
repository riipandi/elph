package skill

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestSlashAgentPromptUsesStructuredActivation(t *testing.T) {
	def := Definition{
		Name:        "aside",
		Description: "Pause for a quick question",
		BaseDir:     "/tmp/aside",
		Body:        "Answer briefly without losing context.",
	}
	got := SlashAgentPrompt(def, "what is X?")
	require.Contains(t, got, `<skill_content name="aside">`)
	require.Contains(t, got, "Answer briefly without losing context.")
	require.Contains(t, got, "<user_args>\nwhat is X?\n</user_args>")
	require.NotContains(t, got, "skill: aside")
}

func TestSlashDetailBodyMatchesAgentPrompt(t *testing.T) {
	def := Definition{
		Name:    "aside",
		BaseDir: "/tmp/aside",
		Body:    "Answer briefly without losing context.",
	}
	require.Equal(t, SlashAgentPrompt(def, "explain mutex"), SlashDetailBody(def, "explain mutex"))
}

func TestSlashDetailLabel(t *testing.T) {
	require.Equal(t, "Skill: aside", SlashDetailLabel("aside"))
}