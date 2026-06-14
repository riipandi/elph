package renderer

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/align"
	"github.com/riipandi/elph/internal/command"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai"
	"github.com/riipandi/elph/pkg/ai/provider"
)

const (
	modelSelectorVisibleModels = 6
	modelSelectorPlaceholder   = "Filter models..."
	modelSelectorCurrentMark   = "‹ "
	modelSelectorCurrentLabel  = "current"
)

var modelSelectorCurrentMarker = lipgloss.NewStyle().Foreground(constants.Green)

// ModelSelectorState tracks the interactive /model picker overlay.
type ModelSelectorState struct {
	Active           bool
	Query            string
	ProviderFilterID string
	Groups           []ai.SelectorGroup
	Flat             []provider.ResolvedModel
	Selected         int
	Scroll           int
	Catalog          provider.Catalog
}

func (m Model) modelSelectorActive() bool {
	return m.modelSelector.Active
}

func (m Model) triggerModelSelector() (Model, tea.Cmd) {
	if m.modelSelectorActive() {
		m = m.closeModelSelector()
		m = m.syncLayout(m.content.AtBottom())
		return m, nil
	}

	catalog, err := provider.LoadCatalog("")
	if err != nil || len(catalog.Providers) == 0 {
		catalog = m.session.Catalog
	}
	if len(catalog.Providers) == 0 {
		return m.withMessage("/model: no providers found — add JSON files to ~/.elph/providers")
	}

	m = m.openModelSelector(catalog, "")
	return m, nil
}

func (m Model) openModelSelector(catalog provider.Catalog, query string) Model {
	groups, flat := ai.BuildSelectorGroups(catalog, query)
	m.modelSelector = ModelSelectorState{
		Active:           true,
		Query:            query,
		ProviderFilterID: "",
		Groups:           groups,
		Flat:             flat,
		Selected:         ai.SelectorPickIndex(flat, m.session.ProviderID, m.session.ModelID),
		Catalog:          catalog,
	}
	m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
	m.input.SetValue(query)
	m.input.Placeholder = m.modelSelectorPlaceholderText()
	m.input.SetHeight(1)
	m.layout.InputScrollTop = 0
	m.showPromptPrefix = false
	m.input.Focus()
	return m.syncLayout(m.content.AtBottom())
}

func (m Model) closeModelSelector() Model {
	m.modelSelector = ModelSelectorState{}
	m.input.SetValue("")
	m.input.Placeholder = ""
	m.showPromptPrefix = false
	m.input.Focus()
	return m
}

func (m Model) refreshModelSelectorItems() Model {
	prev := m.modelSelector.selectedModel()
	m.modelSelector.Query = m.input.Value()
	groups, _ := ai.BuildSelectorGroups(m.modelSelector.Catalog, m.modelSelector.Query)
	m.modelSelector.Groups = groups
	m.modelSelector.ProviderFilterID = ai.NormalizeProviderFilter(m.modelSelector.ProviderFilterID, groups)
	flat := ai.FlattenSelectorGroups(groups, m.modelSelector.ProviderFilterID)

	selected := m.modelSelector.Selected
	if prev.ProviderID != "" {
		selected = ai.SelectorPickIndex(flat, prev.ProviderID, prev.ID)
	} else if selected >= len(flat) {
		if len(flat) > 0 {
			selected = len(flat) - 1
		} else {
			selected = 0
		}
	}

	m.modelSelector.Flat = flat
	m.modelSelector.Selected = selected
	m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
	m.input.Placeholder = m.modelSelectorPlaceholderText()
	return m
}

func (m Model) cycleModelSelectorProvider(delta int) Model {
	if len(m.modelSelector.Groups) == 0 {
		return m
	}

	prev := m.modelSelector.selectedModel()
	m.modelSelector.ProviderFilterID = ai.CycleProviderFilter(m.modelSelector.ProviderFilterID, delta, m.modelSelector.Groups)
	m.modelSelector.Flat = ai.FlattenSelectorGroups(m.modelSelector.Groups, m.modelSelector.ProviderFilterID)

	selected := 0
	if prev.ProviderID != "" {
		selected = ai.SelectorPickIndex(m.modelSelector.Flat, prev.ProviderID, prev.ID)
	}
	m.modelSelector.Selected = selected
	m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
	m.input.Placeholder = m.modelSelectorPlaceholderText()
	return m
}

func (m Model) modelSelectorPlaceholderText() string {
	if !m.modelSelectorActive() {
		return modelSelectorPlaceholder
	}
	for _, group := range m.modelSelector.Groups {
		if group.ProviderID == m.modelSelector.ProviderFilterID {
			return fmt.Sprintf("Filter %s models…", group.ProviderName)
		}
	}
	return modelSelectorPlaceholder
}

func (s ModelSelectorState) selectedModel() provider.ResolvedModel {
	if s.Selected < 0 || s.Selected >= len(s.Flat) {
		return provider.ResolvedModel{}
	}
	return s.Flat[s.Selected]
}

func (m Model) syncModelSelectorScroll(state ModelSelectorState) ModelSelectorState {
	if len(state.Flat) == 0 {
		state.Scroll = 0
		return state
	}
	if state.Selected < 0 {
		state.Selected = 0
	}
	if state.Selected >= len(state.Flat) {
		state.Selected = len(state.Flat) - 1
	}
	if state.Selected < state.Scroll {
		state.Scroll = state.Selected
	}
	if state.Selected >= state.Scroll+modelSelectorVisibleModels {
		state.Scroll = state.Selected - modelSelectorVisibleModels + 1
	}
	maxScroll := max(len(state.Flat)-modelSelectorVisibleModels, 0)
	if state.Scroll > maxScroll {
		state.Scroll = maxScroll
	}
	if state.Scroll < 0 {
		state.Scroll = 0
	}
	return state
}

func (m Model) handleModelSelectorKey(msg tea.KeyPressMsg) (Model, tea.Cmd, bool) {
	if !m.modelSelectorActive() {
		return m, nil, false
	}

	switch {
	case isInputEscapeKey(msg):
		m = m.closeModelSelector()
		m = m.syncLayout(m.content.AtBottom())
		return m, nil, true
	case msg.Code == tea.KeyEnter:
		return m.confirmModelSelector()
	case msg.Code == tea.KeyUp:
		m.modelSelector.Selected = max(m.modelSelector.Selected-1, 0)
		m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
		return m, nil, true
	case msg.Code == tea.KeyDown:
		if len(m.modelSelector.Flat) > 0 {
			m.modelSelector.Selected = min(m.modelSelector.Selected+1, len(m.modelSelector.Flat)-1)
			m.modelSelector = m.syncModelSelectorScroll(m.modelSelector)
		}
		return m, nil, true
	case msg.Code == tea.KeyLeft:
		// Only cycle provider when nothing to navigate in the filter.
		if len(m.input.Value()) == 0 {
			m = m.cycleModelSelectorProvider(-1)
			return m, nil, true
		}
	case msg.Code == tea.KeyRight:
		if len(m.input.Value()) == 0 {
			m = m.cycleModelSelectorProvider(1)
			return m, nil, true
		}
	}
	return m, nil, false
}

func (m Model) confirmModelSelector() (Model, tea.Cmd, bool) {
	if len(m.modelSelector.Flat) == 0 {
		m, cmd := m.withMessage("/model: no models match your search")
		m = m.closeModelSelector()
		m = m.syncLayout(true)
		return m, cmd, true
	}

	model := m.modelSelector.selectedModel()
	regProvider, ok := m.modelSelector.Catalog.Provider(model.ProviderID)
	if !ok {
		m, cmd := m.withMessage(fmt.Sprintf("/model: provider %q not found", model.ProviderID))
		m = m.closeModelSelector()
		m = m.syncLayout(true)
		return m, cmd, true
	}

	cfg, err := provider.SelectModel(m.modelSelector.Catalog, regProvider, model)
	if err != nil {
		m, cmd := m.withMessage(fmt.Sprintf("/model: %v", err))
		m = m.closeModelSelector()
		m = m.syncLayout(true)
		return m, cmd, true
	}

	m = m.applyModelSwitch(&command.ModelSwitch{
		Provider:      cfg.Provider,
		ProviderID:    cfg.ProviderID,
		ProviderName:  cfg.ProviderName,
		ModelID:       cfg.ModelID,
		ModelName:     cfg.ModelName,
		ContextWindow: cfg.ContextWindow,
		MaxTokens:     cfg.MaxTokens,
		Catalog:       m.modelSelector.Catalog,
	})
	m, cmd := m.withMessage(fmt.Sprintf("Switched to %s [%s]", cfg.ModelName, cfg.ProviderName))
	m = m.closeModelSelector()
	m = m.syncLayout(true)
	return m, cmd, true
}

func (m Model) applyModelSwitch(sw *command.ModelSwitch) Model {
	if sw == nil {
		return m
	}
	_ = settings.SetActiveModel(sw.ProviderID, sw.ModelID)
	m.session.Provider = sw.Provider
	m.session.ProviderID = sw.ProviderID
	m.session.ProviderName = sw.ProviderName
	m.session.ModelID = sw.ModelID
	m.session.ModelName = sw.ModelName
	m.session.ContextWindow = sw.ContextWindow
	m.session.MaxTokens = sw.MaxTokens
	m.session.Catalog = sw.Catalog
	m.modelName = sw.ModelName
	m.provider = sw.ProviderName
	m.contextWindow = sw.ContextWindow
	return m
}

// modelSelectorChromeView stacks the model list flush above the filter input,
// sharing a border seam like the slash-command palette.
func (m Model) modelSelectorChromeView() string {
	return lipgloss.JoinVertical(lipgloss.Top, m.modelSelectorListBox(), m.modelSelectorFilterBox())
}

func (m Model) modelSelectorListBox() string {
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	return modelSelectorListBorder(m.mode).Width(boxW).Render(m.modelSelectorListBody())
}

func (m Model) modelSelectorFilterBox() string {
	return m.inputBoxView(true)
}

func (m Model) modelSelectorView() string {
	if !m.modelSelectorActive() {
		return ""
	}
	return m.modelSelectorChromeView()
}

func (m Model) modelSelectorListBody() string {
	flat := m.modelSelector.Flat
	if len(flat) == 0 {
		return dimStyle.Render("No matching models")
	}

	lines := make([]string, 0, len(flat)+1)
	if header := m.modelSelectorProviderHeader(); header != "" {
		lines = append(lines, header)
	}

	names := make([]string, len(flat))
	summaries := make([]string, len(flat))
	for i, model := range flat {
		names[i] = modelDisplayName(model)
		summaries[i] = modelSelectorSummaryPlain(model, m.session.ProviderID, m.session.ModelID)
	}
	nameColW := align.ColumnWidth(names...)

	end := min(m.modelSelector.Scroll+modelSelectorVisibleModels, len(flat))
	for i := m.modelSelector.Scroll; i < end; i++ {
		model := flat[i]
		selected := i == m.modelSelector.Selected

		summaryPlain := summaries[i]
		if i == end-1 && end < len(flat) {
			summaryPlain = fmt.Sprintf("%d more ↓", len(flat)-end)
		}

		var nameStyled string
		if selected {
			nameStyled = cmdPaletteSelected.Render(names[i])
		} else {
			nameStyled = cmdPaletteName.Render(names[i])
		}

		_, gap, _ := align.Row(names[i], nameColW, summaryPlain)
		var summaryStyled string
		if i == end-1 && end < len(flat) {
			if selected {
				summaryStyled = cmdPaletteSummarySelected.Render(summaryPlain)
			} else {
				summaryStyled = dimStyle.Render(summaryPlain)
			}
		} else {
			summaryStyled = modelSelectorSummaryStyled(model, m.session.ProviderID, m.session.ModelID, selected)
		}
		lines = append(lines, modelSelectorLine(nameStyled, gap, summaryStyled))
	}
	return strings.Join(lines, "\n")
}

func (m Model) modelSelectorProviderHeader() string {
	if len(m.modelSelector.Groups) <= 1 {
		return ""
	}
	if m.modelSelector.ProviderFilterID == "" {
		return dimStyle.Render("All providers  ← →")
	}
	for _, group := range m.modelSelector.Groups {
		if group.ProviderID == m.modelSelector.ProviderFilterID {
			return dimStyle.Render(group.ProviderName + "  ← →")
		}
	}
	return ""
}

func modelDisplayName(model provider.ResolvedModel) string {
	if model.Name != "" && model.Name != model.ID {
		return model.Name
	}
	return model.ID
}

func modelSelectorLine(name, gap, summaryStyled string) string {
	return name + gap + summaryStyled
}

func modelProviderName(model provider.ResolvedModel) string {
	if model.ProviderName != "" {
		return model.ProviderName
	}
	return model.ProviderID
}

func modelSelectorSummaryPlain(model provider.ResolvedModel, activeProviderID, activeModelID string) string {
	summary := modelProviderName(model)
	if model.ProviderID == activeProviderID && model.ID == activeModelID {
		return summary + "  " + modelSelectorCurrentMark + modelSelectorCurrentLabel
	}
	return summary
}

func modelSelectorSummaryStyled(model provider.ResolvedModel, activeProviderID, activeModelID string, selected bool) string {
	providerName := modelProviderName(model)
	isCurrent := model.ProviderID == activeProviderID && model.ID == activeModelID

	var providerStyled string
	if selected {
		providerStyled = cmdPaletteSummarySelected.Render(providerName)
	} else {
		providerStyled = dimStyle.Render(providerName)
	}

	if !isCurrent {
		return providerStyled
	}

	marker := modelSelectorCurrentMarker.Render(modelSelectorCurrentMark + modelSelectorCurrentLabel)
	return providerStyled + "  " + marker
}

func (m Model) modelSelectorListHeight() int {
	if !m.modelSelectorActive() {
		return 0
	}
	return lipgloss.Height(m.modelSelectorListBox())
}
