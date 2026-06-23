package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestAppendAgentResponseDeltaStripsToolCalls(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	raw := `<toolcall>
<function=websearch>
<parameter=query>cafe Sukabumi</parameter>
</function>
</toolcall>`

	m = m.appendAgentResponseDelta(raw)

	require.Equal(t, -1, m.agent.ResponseMsgID, "tool-only response should not create AI bubble")
	require.Len(t, m.messages, 1)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Contains(t, m.messages[0].detailLabel, "WebSearch")
	require.Equal(t, uiconst.DetailStatusUnavailable, m.messages[0].detailStatus)
	require.Contains(t, m.messages[0].text, "Tool unavailable")
	require.Contains(t, m.messages[0].text, "cafe Sukabumi")
}

func TestAppendAgentResponseDeltaMalformedAskUserOpensDialog(t *testing.T) {
	m := New()
	m.height = 24
	m.width = 100
	m.ready = true
	m = m.beginAgentTurn()

	raw := `<toolcall><function=AskUser</parameter=options>["English", "Indonesia"]</parameter>` +
		`<parameter=question>What language should the report be in?</parameter></function></toolcall>`

	m = m.appendAgentResponseDelta(raw)
	require.NotNil(t, m.agent.MarkupAskUserPending)
	require.False(t, m.toolInteractDialogActive(), "dialog should wait until the stream turn finishes")

	m, _ = m.finishAgentTurn("", "", nil)
	updated, _ := m.Update(markupAskUserCmdMsg{})
	m = updated.(Model)

	require.True(t, m.toolInteractDialogActive())
	dialog := stripANSI(m.toolInteractChromeView())
	require.Contains(t, dialog, "What language should the report be in")
	require.Contains(t, dialog, "English")
	require.NotContains(t, dialog, "Tool not available")
	require.NotContains(t, dialog, `["English"`)
}

func TestAppendAgentResponseDeltaUnknownTool(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	raw := `<toolcall>
<function=figma_search>
<parameter=query>icons</parameter>
</function>
</toolcall>`

	m = m.appendAgentResponseDelta(raw)

	require.Len(t, m.messages, 1)
	require.Equal(t, "Figma_search", m.messages[0].detailLabel)
	require.Equal(t, uiconst.DetailStatusError, m.messages[0].detailStatus)
	require.Contains(t, m.messages[0].text, "Tool not available")
}

func TestAppendAgentResponseDeltaStripsPartialToolSuffix(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	m = m.appendAgentResponseDelta("Rekomendasi kafe: ")
	m = m.appendAgentResponseDelta("<tool")

	require.Len(t, m.messages, 1)
	require.Equal(t, uiconst.MessageAI, m.messages[0].kind)
	require.Equal(t, "Rekomendasi kafe:", strings.TrimSpace(m.messages[0].text))
}

func TestAppendAgentResponseDeltaIncompleteToolCallCreatesDetailBox(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	raw := `prefix <toolcall><function=websearch><parameter=query>cafe Sukabumi</parameter>`
	m = m.appendAgentResponseDelta(raw)

	require.Len(t, m.messages, 2)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Contains(t, m.messages[0].detailLabel, "WebSearch")
	require.Contains(t, m.messages[0].text, "cafe Sukabumi")
	require.Equal(t, uiconst.MessageAI, m.messages[1].kind)
	require.Equal(t, "prefix", strings.TrimSpace(m.messages[1].text))
}

func TestAppendAgentResponseDeltaStripsLeakedQueryBeforeMarkup(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	query := "rekomendasi tempat ngopi kerja dikota Sukabumi 2024"
	m = m.appendAgentResponseDelta(query)

	raw := `<toolcall>
<function=websearch>
<parameter=query>` + query + `</parameter>
</function>
</toolcall>`
	m = m.appendAgentResponseDelta(raw)

	require.Equal(t, -1, m.agent.ResponseMsgID)
	require.Len(t, m.messages, 1)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Contains(t, m.messages[0].detailLabel, "WebSearch")
}

func TestAppendAgentResponseDeltaStripsMangledUnnamedParameter(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	raw := ` search>
 <parameter>rekomendasi tempat ngopi kerja diSukabumi20240=websearch>bestcafecoworking Sukabumi wifi laptopfriendly0`
	m = m.appendAgentResponseDelta(raw)

	require.Len(t, m.messages, 1)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Contains(t, m.messages[0].detailLabel, "WebSearch")
	require.Contains(t, m.messages[0].text, "Sukabumi")
}

func TestAppendAgentResponseDeltaStripsOrphanToolMarkupTail(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	raw := ` =WebSearch><parameter=query>tempat ngopi kerja dikota Sukabumi cozywifibagus 2024</parameter>
 </function>
 </toolcall>
 </toolcall>`
	m = m.appendAgentResponseDelta(raw)

	require.Len(t, m.messages, 1)
	require.Equal(t, uiconst.MessageDetail, m.messages[0].kind)
	require.Contains(t, m.messages[0].detailLabel, "WebSearch")
	require.Contains(t, m.messages[0].text, "Sukabumi")
}

func TestAppendAgentResponseDeltaStripsToolCallUnderscoreClose(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()

	raw := `<toolcall>
<function=websearch>
<parameter=query>rekomendasi tempat ngopi sambil bekerja coworking cafe Sukabumi kota 2024</parameter>
</function>
</tool_call>`

	m = m.appendAgentResponseDelta(raw)

	require.Equal(t, -1, m.agent.ResponseMsgID)
	require.Len(t, m.messages, 1)
	require.Contains(t, m.messages[0].detailLabel, "WebSearch")
}

func TestFinishAgentTurnStripsEmbeddedToolCalls(t *testing.T) {
	m := New()
	m = m.beginAgentTurn()
	m.messages = []message{{text: "partial", kind: uiconst.MessageAI}}
	m.agent.ResponseMsgID = 0

	response := `Here are ideas.<toolcall>
<function=websearch>
<parameter=query>coworking Sukabumi</parameter>
</function>
</toolcall>`

	m, _ = m.finishAgentTurn("", response, nil)

	require.Len(t, m.messages, 2)
	require.Equal(t, "Here are ideas.", m.messages[0].text)
	require.Contains(t, m.messages[1].detailLabel, "WebSearch")
}
