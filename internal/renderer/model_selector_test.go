package renderer

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func writeProviderFile(t *testing.T, dir, name, body string) {
	t.Helper()
	require.NoError(t, os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644))
}

func testModelCatalog(t *testing.T) provider.Catalog {
	t.Helper()
	dir := t.TempDir()
	writeProviderFile(t, dir, "alpha.json", `{
		"name": "Alpha",
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": [{"id": "a1", "name": "Alpha One", "contextWindow": 128000, "maxTokens": 8192}]
	}`)
	writeProviderFile(t, dir, "beta.json", `{
		"name": "Beta",
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": [
			{"id": "b1", "name": "Beta One", "contextWindow": 128000, "maxTokens": 8192},
			{"id": "b2", "name": "Beta Two", "contextWindow": 200000, "maxTokens": 16384}
		]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)
	return catalog
}

func TestModelSelectorOpensFlatList(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)

	m = m.openModelSelector(catalog, "")
	require.True(t, m.modelSelectorActive())
	require.Len(t, m.modelSelector.Flat, 3)

	view := stripANSI(m.modelSelectorView())
	require.Contains(t, view, "Alpha One")
	require.Contains(t, view, "Beta Two")
	require.Contains(t, view, "Alpha")
	require.Contains(t, view, "Beta")
	require.NotContains(t, view, "128k")
	require.NotContains(t, view, "200k")
	require.NotContains(t, view, "move")
	require.Contains(t, view, "Filter models")
}

func TestModelSelectorMarksCurrentModelOnRight(t *testing.T) {
	m := testInputModel(t)
	m.session.ProviderID = "beta"
	m.session.ModelID = "b2"
	m.session.ModelName = "Beta Two"

	m = m.openModelSelector(testModelCatalog(t), "")
	view := m.modelSelectorView()
	plain := stripANSI(view)

	require.Contains(t, plain, "Beta  ‹ current")
	require.Contains(t, view, modelSelectorCurrentMarker.Render(modelSelectorCurrentMark+modelSelectorCurrentLabel))
}

func TestModelSelectorLayoutFitsTerminal(t *testing.T) {
	m := testInputModel(t)
	idleRendered := m.renderedViewHeight()
	idleChrome := m.layout.ChromeH
	idleContent := m.content.Height()

	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")

	require.LessOrEqual(t, m.renderedViewHeight(), m.height)
	require.Equal(t, idleRendered, m.renderedViewHeight(), "frame should still fill terminal height")
	require.Greater(t, m.layout.ChromeH, idleChrome)
	require.Less(t, m.content.Height(), idleContent)
	require.Equal(t, m.content.Height()+m.layout.ChromeH, m.height)
}

func TestModelSelectorChromeStacksListAndFilter(t *testing.T) {
	m := testInputModel(t)
	m = m.openModelSelector(testModelCatalog(t), "")

	selectorChrome := lipgloss.Height(m.inputChromeView())
	listH := m.modelSelectorListHeight()
	filterH := lipgloss.Height(m.modelSelectorFilterBox())
	inputH := lipgloss.Height(m.inputBoxView(true))

	require.Equal(t, listH+filterH, selectorChrome)
	require.Equal(t, inputH, filterH)
	require.Greater(t, listH, lipgloss.Height(m.commandPaletteView()),
		"model list keeps a little bottom padding inside the shared chrome")
}

func TestModelSelectorFilterBelowList(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")

	listH := m.modelSelectorListHeight()
	filterH := lipgloss.Height(m.modelSelectorFilterBox())
	require.Equal(t, listH+filterH, lipgloss.Height(m.modelSelectorView()))

	view := stripANSI(m.modelSelectorView())
	require.Greater(t, strings.Index(view, "Filter models"), strings.Index(view, "Alpha One"))
}

func TestModelSelectorFilterViaInput(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")

	for _, ch := range "two" {
		m.input, _ = m.input.Update(keyRune(ch))
	}
	updated, _ := m.finalizeInputEdit()

	require.Len(t, updated.modelSelector.Flat, 1)
	require.Equal(t, "b2", updated.modelSelector.Flat[0].ID)
	require.Equal(t, "two", updated.input.Value())
}

func TestModelSelectorFuzzyFilter(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)

	m = m.openModelSelector(catalog, "")
	m.input.SetValue("two")
	m = m.refreshModelSelectorItems()

	require.Len(t, m.modelSelector.Flat, 1)
	require.Equal(t, "b2", m.modelSelector.Flat[0].ID)
}

func TestModelSelectorProviderFilterWithArrows(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")
	require.Len(t, m.modelSelector.Flat, 3)

	updated, _, handled := m.handleModelSelectorKey(keyRight())
	require.True(t, handled)
	require.Equal(t, "alpha", updated.modelSelector.ProviderFilterID)
	require.Len(t, updated.modelSelector.Flat, 1)
	require.Equal(t, "a1", updated.modelSelector.Flat[0].ID)
	require.Equal(t, "Filter Alpha models...", updated.input.Placeholder)

	updated, _, handled = updated.handleModelSelectorKey(keyRight())
	require.True(t, handled)
	require.Equal(t, "beta", updated.modelSelector.ProviderFilterID)
	require.Len(t, updated.modelSelector.Flat, 2)

	updated, _, handled = updated.handleModelSelectorKey(keyRight())
	require.True(t, handled)
	require.Empty(t, updated.modelSelector.ProviderFilterID)
	require.Len(t, updated.modelSelector.Flat, 3)

	updated, _, handled = updated.handleModelSelectorKey(keyLeft())
	require.True(t, handled)
	require.Equal(t, "beta", updated.modelSelector.ProviderFilterID)
	require.Len(t, updated.modelSelector.Flat, 2)
}

func TestModelSelectorNavigation(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")

	m.modelSelector.Selected = 0
	updated, _, handled := m.handleModelSelectorKey(keyDown())
	require.True(t, handled)
	require.Equal(t, 1, updated.modelSelector.Selected)
	require.Equal(t, "b1", updated.modelSelector.Flat[1].ID)
}

func TestModelSelectorConfirmWithoutAPIKeyPersistsSelection(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := t.TempDir()
	writeProviderFile(t, dir, "demo.json", `{
		"name": "Demo",
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "",
		"models": [{"id": "m1", "name": "Demo Model", "contextWindow": 128000, "maxTokens": 8192}]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	m := testInputModel(t)
	m = m.openModelSelector(catalog, "")

	updated, _, handled := m.confirmModelSelector()
	require.True(t, handled)
	require.False(t, updated.modelSelectorActive())
	require.Nil(t, updated.session.Provider)
	require.Equal(t, "demo", updated.session.ProviderID)
	require.Equal(t, "m1", updated.session.ModelID)
	require.Contains(t, updated.messages[len(updated.messages)-1].text, "apiKey")

	cfg, err := settings.Load()
	require.NoError(t, err)
	require.Equal(t, "demo", cfg.ActiveProviderID())
	require.Equal(t, "m1", cfg.ActiveModelID())
}

func TestModelSelectorConfirmPersistsSettings(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")

	for i, model := range m.modelSelector.Flat {
		if model.ID == "b2" {
			m.modelSelector.Selected = i
			break
		}
	}

	updated, _, handled := m.confirmModelSelector()
	require.True(t, handled)

	cfg, err := settings.Load()
	require.NoError(t, err)
	require.Equal(t, "beta", cfg.ActiveProviderID())
	require.Equal(t, "b2", cfg.ActiveModelID())
	require.Equal(t, "beta", updated.session.ProviderID)
}

func TestModelSelectorConfirmSwitchesModel(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m = m.openModelSelector(catalog, "")

	for i, model := range m.modelSelector.Flat {
		if model.ID == "b2" {
			m.modelSelector.Selected = i
			break
		}
	}

	updated, _, handled := m.confirmModelSelector()
	require.True(t, handled)
	require.False(t, updated.modelSelectorActive())
	require.Equal(t, "b2", updated.session.ModelID)
	require.Equal(t, "Beta Two", updated.session.ModelName)
	require.Equal(t, "beta", updated.session.ProviderID)
}

func TestModelSelectorEscapeCloses(t *testing.T) {
	m := testInputModel(t)
	m = m.openModelSelector(testModelCatalog(t), "")
	require.True(t, m.modelSelectorActive())

	updated, cmd := m.Update(keyEscape())
	m = updated.(Model)
	require.Nil(t, cmd)
	require.False(t, m.modelSelectorActive())
	require.Empty(t, m.input.Value())
}

func TestCtrlLOpensModelSelector(t *testing.T) {
	m := testInputModel(t)
	m.session.Catalog = testModelCatalog(t)

	updated, cmd := m.Update(keyCtrl('l'))
	m = updated.(Model)
	require.Nil(t, cmd)
	require.True(t, m.modelSelectorActive())
	require.Equal(t, modelSelectorPlaceholder, m.input.Placeholder)
}

func TestCtrlLTogglesModelSelector(t *testing.T) {
	m := testInputModel(t)
	m.session.Catalog = testModelCatalog(t)

	updated, _ := m.Update(keyCtrl('l'))
	m = updated.(Model)
	require.True(t, m.modelSelectorActive())

	updated, _ = m.Update(keyCtrl('l'))
	m = updated.(Model)
	require.False(t, m.modelSelectorActive())
}

func TestModelSelectorSingleProviderHidesRepeatedProviderColumn(t *testing.T) {
	m := testInputModel(t)
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"name": "OpenCode Go",
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": [
			{"id": "qwen", "name": "Qwen3.5 Plus", "contextWindow": 128000, "maxTokens": 8192},
			{"id": "mimo", "name": "MiMo V2 Pro", "contextWindow": 128000, "maxTokens": 8192},
			{"id": "hy3", "name": "hy3-preview", "contextWindow": 128000, "maxTokens": 8192}
		]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	m.session.ProviderID = "opencode"
	m.session.ModelID = "mimo"
	m = m.openModelSelector(catalog, "")

	view := stripANSI(m.modelSelectorView())
	require.Contains(t, view, "OpenCode Go")
	require.Contains(t, view, "Qwen3.5 Plus")
	require.Contains(t, view, "MiMo V2 Pro")
	require.Contains(t, view, "‹ current")
	require.NotContains(t, view, "Qwen3.5 Plus     OpenCode Go")
	require.NotContains(t, view, "hy3-preview     OpenCode Go")
}

func TestModelSelectorOverflowOnSeparateLine(t *testing.T) {
	m := testInputModel(t)
	dir := t.TempDir()
	modelsJSON := strings.Builder{}
	modelsJSON.WriteString("[")
	for i := 0; i < 10; i++ {
		if i > 0 {
			modelsJSON.WriteString(",")
		}
		fmt.Fprintf(&modelsJSON, `{"id": "m%d", "name": "Model %d", "contextWindow": 128000, "maxTokens": 8192}`, i, i)
	}
	modelsJSON.WriteString("]")
	writeProviderFile(t, dir, "solo.json", fmt.Sprintf(`{
		"name": "Solo",
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": %s
	}`, modelsJSON.String()))
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	m = m.openModelSelector(catalog, "")
	view := stripANSI(m.modelSelectorView())
	require.Contains(t, view, "4 more ↓")
	require.Contains(t, view, "Model 5")
	require.NotContains(t, view, "Model 5     4 more")
}

func TestSlashModelOpensSelector(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m.session.Catalog = catalog

	m.input.SetValue("/model beta")
	updated, _, handled := m.handleSlashCommand("/model beta")
	require.True(t, handled)
	require.True(t, updated.modelSelectorActive())
	require.Equal(t, "beta", updated.modelSelector.Query)
}

func TestModelSlashDraftNotRestoredAfterConfirm(t *testing.T) {
	m := testInputModel(t)
	catalog := testModelCatalog(t)
	m.session.Catalog = catalog
	m.session.Provider = nil
	m.session.ProviderID = ""
	m.session.ModelID = ""

	m.input.SetValue("/model")
	updated, _ := m.Update(keyCtrl('l'))
	m = updated.(Model)
	require.True(t, m.modelSelectorActive())
	require.Nil(t, m.pendingPromptDraft)

	m, _, handled := m.confirmModelSelector()
	require.True(t, handled)
	require.False(t, m.modelSelectorActive())
	require.Empty(t, m.input.Value())

	m.input.SetValue("hello")
	m, _, ok := m.trySubmitInput()
	require.True(t, ok)
	require.False(t, m.modelSelectorActive())
}

func TestSubmitAfterModelSelectWithoutCredentials(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := t.TempDir()
	writeProviderFile(t, dir, "demo.json", `{
		"name": "Demo",
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "",
		"models": [{"id": "m1", "name": "Demo Model", "contextWindow": 128000, "maxTokens": 8192}]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	m := testInputModel(t)
	m.session.Catalog = catalog
	m.session.Provider = nil
	m.session.ProviderID = ""
	m.session.ModelID = ""

	m = m.openModelSelector(catalog, "")
	updated, _, handled := m.confirmModelSelector()
	require.True(t, handled)
	require.False(t, updated.modelSelectorActive())
	require.Nil(t, updated.session.Provider)
	require.Equal(t, "demo", updated.session.ProviderID)
	require.True(t, updated.hasActiveModel())

	updated.input.SetValue("hello")
	updated, _, ok := updated.trySubmitInput()
	require.True(t, ok)
	require.False(t, updated.modelSelectorActive())
	require.True(t, updated.agent.Busy)
}
