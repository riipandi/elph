package renderer

import (
	"fmt"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/pkg/ai/provider"
)

const modelsSyncUpdatingLabel = "Updating model metadata from models.dev"

type modelsSyncDueMsg struct{}

type modelsSyncDoneMsg struct {
	err    error
	result provider.UpdateModelsResult
}

func checkModelsSyncDueCmd() tea.Cmd {
	return func() tea.Msg {
		cfg, err := settings.Load()
		if err != nil {
			return modelsSyncDoneMsg{err: err}
		}
		if !cfg.SyncDue(time.Now()) {
			return nil
		}
		return modelsSyncDueMsg{}
	}
}

func runModelsSyncCmd() tea.Cmd {
	return func() tea.Msg {
		result, err := settings.RunModelsSync()
		return modelsSyncDoneMsg{err: err, result: result}
	}
}

func (m Model) modelsSyncingActive() bool {
	return m.modelsSyncing
}

func (m Model) modelsSyncStatusText() string {
	frame := spinnerFrames[m.agent.SpinnerFrame%len(spinnerFrames)]
	return frame + " " + modelsSyncUpdatingLabel + "…"
}

func (m Model) startModelsSync() (Model, tea.Cmd) {
	m.modelsSyncing = true
	m.agent.SpinnerFrame = 0
	m, _ = m.setModelsSyncMessage(m.modelsSyncStatusText())
	return m, tea.Batch(runModelsSyncCmd(), m.spinnerTickCmd())
}

func (m Model) setModelsSyncMessage(text string) (Model, tea.Cmd) {
	msg := message{text: text, kind: constants.MessageSystem}
	if m.modelsSyncMsgID >= 0 && m.modelsSyncMsgID < len(m.messages) {
		m.messages[m.modelsSyncMsgID] = msg
	} else {
		m.messages = append(m.messages, msg)
		m.modelsSyncMsgID = len(m.messages) - 1
	}
	m.layout.ContentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) refreshModelsSyncStatus() Model {
	if !m.modelsSyncing {
		return m
	}
	m, _ = m.setModelsSyncMessage(m.modelsSyncStatusText())
	return m
}

func (m Model) finishModelsSync(msg modelsSyncDoneMsg) Model {
	m.modelsSyncing = false
	text := formatModelsSyncResult(msg)
	m, _ = m.setModelsSyncMessage(text)
	m.session.AppendLog("system", text)
	m.modelsSyncMsgID = -1

	if msg.err != nil {
		return m
	}

	catalog, err := provider.LoadCatalog("")
	if err != nil {
		return m
	}
	m.session.Catalog = catalog
	return m
}

func formatModelsSyncResult(msg modelsSyncDoneMsg) string {
	if msg.err != nil {
		return fmt.Sprintf("Model metadata update failed: %v", msg.err)
	}
	if len(msg.result.Updated) > 0 {
		return "Model metadata updated: " + strings.Join(msg.result.Updated, ", ")
	}
	return "Model metadata is up to date."
}
