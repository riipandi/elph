package renderer

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/skill"
	"github.com/stretchr/testify/require"
)

func TestSkillSlashShowsSkillDetailBox(t *testing.T) {
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
	require.Len(t, model.messages, 3)
	require.Equal(t, constants.MessageUser, model.messages[0].kind)
	require.Equal(t, "/skill:aside explain mutex", model.messages[0].text)
	require.Equal(t, constants.MessageDetail, model.messages[1].kind)
	require.Equal(t, "Skill: aside", model.messages[1].detailLabel)
	require.Contains(t, model.messages[1].text, "Answer briefly without losing context.")
	require.Contains(t, model.messages[1].text, "<user_args>\nexplain mutex\n</user_args>")
	require.False(t, model.messages[1].detailExpanded)
	require.Equal(t, constants.MessageThinking, model.messages[2].kind)
}
