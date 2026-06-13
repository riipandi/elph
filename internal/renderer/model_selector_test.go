package renderer

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
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

	for _, ch := range []rune("two") {
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
	require.Equal(t, "Filter Alpha models…", updated.input.Placeholder)

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
