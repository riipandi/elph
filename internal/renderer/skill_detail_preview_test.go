package renderer

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/skill"
	"github.com/stretchr/testify/require"
)

func TestSkillDetailCollapsedPreviewShowsInstructions(t *testing.T) {
	def := skill.Definition{Body: "Answer briefly without losing context."}
	body := skill.SlashDetailBody(def, "explain mutex")

	m := testModel()
	m.messages = []message{{
		kind:        constants.MessageDetail,
		detailLabel: "Skill: aside",
		text:        body,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Answer briefly without losing context.")
	require.NotContains(t, rendered, "<skill_content")
}

func TestSkillDetailCollapsedByDefaultOnSubmit(t *testing.T) {
	m := testInputModel(t)

	skillDir := filepath.Join(os.Getenv("HOME"), ".elph", "skills", "aside")
	require.NoError(t, os.MkdirAll(skillDir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(skillDir, skill.FileName), []byte(`---
name: aside
description: Pause for a quick question
---
Answer briefly without losing context.`), 0o644))

	m.input.SetValue("/skill:aside explain mutex")

	updated, cmd := m.Update(keyEnter())
	model := updated.(Model)
	require.NotNil(t, cmd)
	require.False(t, model.messages[1].detailExpanded)
	require.Equal(t, "Skill: aside", model.messages[1].detailLabel)
	require.Contains(t, model.messages[1].text, "Answer briefly without losing context.")
	require.Contains(t, model.messages[1].text, "<user_args>\nexplain mutex\n</user_args>")

	rendered := stripANSI(model.renderMessageAt(1))
	require.Contains(t, rendered, "ctrl+o to expand")
	require.NotContains(t, rendered, "explain mutex")
}

func TestFirstDetailLineSkipsMarkdownHeadersAndPreamble(t *testing.T) {
	require.Equal(t, "real content", firstDetailLine("## Instructions\nreal content\nmore"))
	require.Equal(t, "args", firstDetailLine("User prompt:\nargs"))
	require.Equal(t, "body line", firstDetailLine(`<skill_content name="aside">`+"\n\nbody line"))
	require.Equal(t, "Answer briefly.", firstDetailLine(
		`<skill_content name="aside">`+"\n"+
			"Apply this skill's workflow internally. User-visible output must follow system prompt Output rules.\n\n"+
			"Answer briefly.\n",
	))
}