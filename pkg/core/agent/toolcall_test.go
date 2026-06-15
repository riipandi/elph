package agent

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestStripToolCallsSingle(t *testing.T) {
	raw := `Hello<toolcall>
<function=websearch>
<parameter=query>cafe Sukabumi</parameter>
</function>
</toolcall> world`

	clean, calls := StripToolCalls(raw)
	require.Equal(t, "Hello world", clean)
	require.Len(t, calls, 1)
	require.Equal(t, "websearch", calls[0].Name)
	require.Equal(t, "cafe Sukabumi", calls[0].Parameters["query"])
}

func TestStripToolCallsMultipleAdjacent(t *testing.T) {
	raw := `<toolcall>
<function=websearch>
<parameter=query>one</parameter>
</function>
</toolcall><toolcall>
<function=WebSearch>
<parameter=query>two</parameter>
</function>
</toolcall>`

	clean, calls := StripToolCalls(raw)
	require.Empty(t, clean)
	require.Len(t, calls, 2)
	require.Equal(t, "one", calls[0].Parameters["query"])
	require.Equal(t, "two", calls[1].Parameters["query"])
}

func TestToolCallStreamFilterHoldsIncompleteBlock(t *testing.T) {
	var f ToolCallStreamFilter

	safe, calls := f.Process("prefix <toolcall><function=read>")
	require.Equal(t, "prefix", safe)
	require.Empty(t, calls)

	safe, calls = f.Process("<parameter=path>/tmp/a</parameter></function></toolcall> tail")
	require.Equal(t, "tail", safe)
	require.Len(t, calls, 1)
	require.Equal(t, "read", calls[0].Name)
	require.Equal(t, "/tmp/a", calls[0].Parameters["path"])
}

func TestStripToolCallsToolCallUnderscoreClose(t *testing.T) {
	raw := `<toolcall>
<function=websearch>
<parameter=query>coworking cafe Sukabumi kota 2024</parameter>
</function>
</tool_call>`

	clean, calls := StripToolCalls(raw)
	require.Empty(t, clean)
	require.Len(t, calls, 1)
	require.Equal(t, "websearch", calls[0].Name)
	require.Contains(t, calls[0].Parameters["query"], "Sukabumi")
}

func TestStripToolCallsUnderscoreOpenAndClose(t *testing.T) {
	raw := `<tool_call>
<function=WebSearch>
<parameter=query>test</parameter>
</function>
</tool_call>`

	clean, calls := StripToolCalls(raw)
	require.Empty(t, clean)
	require.Len(t, calls, 1)
	require.Equal(t, "WebSearch", calls[0].Name)
}

func TestStripToolCallsPartialToolSuffix(t *testing.T) {
	clean, calls := StripToolCalls("Berikut rekomendasinya. <tool")
	require.Equal(t, "Berikut rekomendasinya.", clean)
	require.Empty(t, calls)

	clean, calls = StripToolCalls("<tool")
	require.Empty(t, clean)
	require.Empty(t, calls)

	clean, calls = StripToolCalls("prefix <toolcall><function=websearch><parameter=query>cafe</parameter>")
	require.Equal(t, "prefix", clean)
	require.Len(t, calls, 1)
	require.Equal(t, "websearch", calls[0].Name)
}

func TestStripToolCallsLooseFunctionMarkup(t *testing.T) {
	raw := `Checking.<function=Grep>
<parameter=pattern>foo</parameter>
</function> Done.`

	clean, calls := StripToolCalls(raw)
	require.Equal(t, "Checking. Done.", clean)
	require.Len(t, calls, 1)
	require.Equal(t, "Grep", calls[0].Name)
}

func TestSanitizeAssistantDisplayRemovesPartialTool(t *testing.T) {
	require.Equal(t, "prefix", SanitizeAssistantDisplay("prefix <toolcall"))
	require.Equal(t, "hello", SanitizeAssistantDisplay("hello <tool"))
}

func TestStripToolCallsMangledUnnamedParameter(t *testing.T) {
	raw := ` search>
 <parameter>rekomendasi tempat ngopi kerja diSukabumi20240=websearch>bestcafecoworking Sukabumi wifi laptopfriendly0`

	clean, calls := StripToolCalls(raw)
	require.Empty(t, clean)
	require.Len(t, calls, 1)
	require.Equal(t, "WebSearch", calls[0].Name)
	require.Contains(t, calls[0].Parameters["query"], "rekomendasi")
	require.Contains(t, calls[0].Parameters["query"], "Sukabumi")
}

func TestStripToolCallsDoesNotStripNormalProse(t *testing.T) {
	raw := "Saya sarankan cari kafe dengan wifi bagus di Sukabumi pada 2024."
	clean, calls := StripToolCalls(raw)
	require.Equal(t, raw, clean)
	require.Empty(t, calls)
}

func TestStripToolCallsPreservesParagraphBreaks(t *testing.T) {
	raw := "First paragraph.\n\nSecond paragraph."
	clean, calls := StripToolCalls(raw)
	require.Equal(t, raw, clean)
	require.Empty(t, calls)
}

func TestToolCallStreamFilterMangledUnnamedParameter(t *testing.T) {
	var f ToolCallStreamFilter
	raw := ` search>
 <parameter>rekomendasi tempat ngopi kerja diSukabumi20240=websearch>bestcafecoworking Sukabumi wifi laptopfriendly0`
	safe, calls := f.Process(raw)
	require.Empty(t, safe)
	require.Len(t, calls, 1)
	require.Equal(t, "WebSearch", calls[0].Name)
}

func TestStripToolCallsOrphanClosingTags(t *testing.T) {
	raw := ` =WebSearch><parameter=query>tempat ngopi kerja dikota Sukabumi cozywifibagus 2024</parameter>
 </function>
 </toolcall>
 </toolcall>`

	clean, calls := StripToolCalls(raw)
	require.Empty(t, clean)
	require.Len(t, calls, 1)
	require.Equal(t, "WebSearch", calls[0].Name)
	require.Contains(t, calls[0].Parameters["query"], "Sukabumi")
}

func TestToolCallStreamFilterStripsOrphanMarkupTail(t *testing.T) {
	var f ToolCallStreamFilter
	safe, calls := f.Process(` =WebSearch><parameter=query>cafe Sukabumi</parameter>
</function>
</toolcall>
</toolcall>`)
	require.Empty(t, safe)
	require.Len(t, calls, 1)
	require.Equal(t, "WebSearch", calls[0].Name)
}

func TestToolCallStreamFilterHoldsSplitFunctionTag(t *testing.T) {
	var f ToolCallStreamFilter
	safe, _ := f.Process("prefix <toolcall>\n<function")
	require.Equal(t, "prefix", safe)

	safe, calls := f.Process("=WebSearch><parameter=query>cafe</parameter></function></toolcall>")
	require.Empty(t, safe)
	require.Len(t, calls, 1)
	require.Equal(t, "WebSearch", calls[0].Name)
}

func TestToolCallStreamFilterFlushIgnoresStaleHoldback(t *testing.T) {
	var f ToolCallStreamFilter
	safe, _ := f.Process("prefix <tool")
	require.Equal(t, "prefix", safe)

	full := `<toolcall>
<function=websearch>
<parameter=query>coworking cafe Sukabumi kota 2024</parameter>
</function>
</tool_call>`
	clean, calls := f.Flush(full)
	require.Empty(t, clean)
	require.Len(t, calls, 1)
}

func TestToolCallStreamFilterFlushTrailingHoldback(t *testing.T) {
	var f ToolCallStreamFilter
	safe, _ := f.Process("before <toolcall><function=grep>")
	require.Equal(t, "before", safe)

	clean, calls := f.Flush("<parameter=pattern>foo</parameter></function></toolcall> after")
	require.Equal(t, "after", clean)
	require.Len(t, calls, 1)
	require.Equal(t, "grep", calls[0].Name)
}
