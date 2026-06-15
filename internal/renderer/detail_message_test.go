package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/prompt/template"
	"github.com/riipandi/elph/internal/settings"
	"github.com/stretchr/testify/require"
)

func TestDetailMessageCollapsedByDefault(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:        constants.MessageDetail,
		detailLabel: "Prompt",
		text:        "Analyze this codebase.\nFocus on: auth",
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Prompt")
	require.Contains(t, rendered, "ctrl+o to expand")
	require.NotContains(t, rendered, "ctrl+o to collapse")
	require.Contains(t, rendered, "Analyze this codebase.")
	require.NotContains(t, rendered, "Focus on: auth")
}

func TestDetailMessageExpandedShowsFullBody(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		kind:           constants.MessageDetail,
		detailLabel:    "Prompt",
		text:           "Analyze this codebase.\nFocus on: auth",
		detailExpanded: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Focus on: auth")
}

func TestToggleLastDetailExpand(t *testing.T) {
	m := testModel()
	m.messages = []message{
		{kind: constants.MessageUser, text: "hello"},
		{
			kind:        constants.MessageDetail,
			detailLabel: "Prompt",
			text:        "Identify the codebase.\nFocus on architecture.",
		},
	}

	collapsed := stripANSI(m.renderMessageAt(1))
	m, _ = m.toggleLastDetailExpand()
	expanded := stripANSI(m.renderMessageAt(1))
	require.NotEqual(t, collapsed, expanded)
	require.Greater(t, lipgloss.Height(expanded), lipgloss.Height(collapsed))
}

func TestPromptTemplateShowsSeparateUserAndDetailMessages(t *testing.T) {
	m := testInputModel(t)
	m.promptTemplates = []template.Template{{
		Name:    "identify",
		Content: "Identify the codebase focusing on $1.",
	}}
	m.input.SetValue("/identify auth")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)
	require.NotNil(t, cmd)
	require.Len(t, m.messages, 3)
	require.Equal(t, constants.MessageUser, m.messages[0].kind)
	require.Equal(t, "/identify auth", m.messages[0].text)
	require.Equal(t, constants.MessageDetail, m.messages[1].kind)
	require.Equal(t, "Prompt", m.messages[1].detailLabel)
	require.Contains(t, m.messages[1].text, "focusing on auth")
	require.Equal(t, constants.MessageThinking, m.messages[2].kind)
}

func TestCtrlOTogglesDetailBlock(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{{
		kind:        constants.MessageDetail,
		detailLabel: "$ ls",
		text:        "file.txt\nREADME.md",
	}}

	collapsed := stripANSI(m.renderMessageAt(0))
	updated, cmd := m.Update(keyCtrl('o'))
	m = updated.(Model)
	require.Nil(t, cmd)
	expanded := stripANSI(m.renderMessageAt(0))
	require.NotEqual(t, collapsed, expanded)
	require.Contains(t, expanded, "README.md")
}

func TestCtrlOUpdatesContentViewWhileStreaming(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{
		{kind: constants.MessageUser, text: "/identify auth"},
		{
			kind:        constants.MessageDetail,
			detailLabel: "Prompt",
			text:        "line one\nline two\nline three",
		},
		{kind: constants.MessageAI, text: "partial response"},
	}
	m.agent.Busy = true
	m.agent.ResponseMsgID = 2
	m = m.refreshStreamPrefixCache()

	before := stripANSI(viewContent(m))
	updated, cmd := m.Update(keyCtrl('o'))
	m = updated.(Model)
	require.Nil(t, cmd)
	after := stripANSI(viewContent(m))
	require.NotEqual(t, before, after)
	require.Contains(t, after, "line two")
}

func TestThinkingMessageCollapsedByDefault(t *testing.T) {
	m := testInputModel(t)
	m = m.addThinkingMessage("reasoning step one\nreasoning step two")

	require.False(t, m.messages[0].detailExpanded)
	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Thinking")
	require.Contains(t, rendered, "ctrl+o to expand")
	require.Contains(t, rendered, "reasoning step one")
	require.NotContains(t, rendered, "reasoning step two")
}

func TestThinkingAutoExpandSetting(t *testing.T) {
	m := testInputModel(t)
	enabled := true
	require.NoError(t, settings.Save(settings.Settings{
		SyncInterval:       "24h",
		AutoExpandThinking: &enabled,
	}))
	m = m.addThinkingMessage("expanded reasoning\nsecond line")
	require.True(t, m.messages[0].detailExpanded)

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "ctrl+o to collapse")
	require.Contains(t, rendered, "second line")
}

func TestCtrlOTogglesThinkingBlock(t *testing.T) {
	m := testInputModel(t)
	m = m.addThinkingMessage("alpha\nbeta\ngamma")

	collapsed := stripANSI(m.renderMessageAt(0))
	updated, cmd := m.Update(keyCtrl('o'))
	m = updated.(Model)
	require.Nil(t, cmd)
	expanded := stripANSI(m.renderMessageAt(0))
	require.NotEqual(t, collapsed, expanded)
	require.Contains(t, expanded, "beta")
	require.Contains(t, expanded, "ctrl+o to collapse")
}

func TestDetailMessageDiffersFromAIStyle(t *testing.T) {
	detailStyle := constants.DetailStatusStyle(constants.DetailStatusNeutral)
	aiStyle := constants.MessageStyle(constants.MessageAI)
	require.NotEqual(t, detailStyle.GetBackground(), aiStyle.GetBackground())
}

func TestDetailTitleHasNoBackgroundChip(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{
		kind:         constants.MessageDetail,
		detailLabel:  "Prompt",
		text:         "body",
		detailStatus: constants.DetailStatusNeutral,
	})
	firstLine := strings.SplitN(rendered, "\n", 2)[0]
	require.Contains(t, stripANSI(firstLine), "Prompt")
	require.NotContains(t, firstLine, "\x1b[48")
}
