package renderer

import (
	"fmt"
	"reflect"
	"time"

	"charm.land/bubbles/v2/stopwatch"
	tea "charm.land/bubbletea/v2"
)

func newActivityStopwatch() stopwatch.Model {
	return stopwatch.New(stopwatch.WithInterval(200 * time.Millisecond))
}

func formatCompactElapsed(d time.Duration) string {
	switch {
	case d < time.Second:
		return fmt.Sprintf("%dms", d.Milliseconds())
	case d < time.Minute:
		secs := d.Seconds()
		if secs < 10 {
			return fmt.Sprintf("%.1fs", secs)
		}
		return fmt.Sprintf("%ds", int(secs+0.5))
	default:
		mins := int(d.Minutes())
		secs := int(d.Seconds()) % 60
		return fmt.Sprintf("%dm%02ds", mins, secs)
	}
}

func applyActivityStopwatchCmd(sw stopwatch.Model, cmd tea.Cmd) stopwatch.Model {
	for _, msg := range drainTeaCmd(cmd) {
		sw, _ = sw.Update(msg)
	}
	return sw
}

func drainTeaCmd(cmd tea.Cmd) []tea.Msg {
	if cmd == nil {
		return nil
	}
	return drainTeaMsg(cmd())
}

func drainTeaMsg(msg tea.Msg) []tea.Msg {
	if msg == nil {
		return nil
	}
	switch batch := msg.(type) {
	case tea.BatchMsg:
		var msgs []tea.Msg
		for _, c := range batch {
			msgs = append(msgs, drainTeaCmd(c)...)
		}
		return msgs
	}
	val := reflect.ValueOf(msg)
	if val.Kind() == reflect.Slice && val.Type().Elem().Kind() == reflect.Func {
		var msgs []tea.Msg
		for i := 0; i < val.Len(); i++ {
			if c, ok := val.Index(i).Interface().(tea.Cmd); ok {
				msgs = append(msgs, drainTeaCmd(c)...)
			}
		}
		if len(msgs) > 0 {
			return msgs
		}
	}
	return []tea.Msg{msg}
}

func (m Model) activityStopwatchStartCmd() tea.Cmd {
	return tea.Batch(
		m.agent.Stopwatch.Reset(),
		m.agent.Stopwatch.Start(),
	)
}

func (m Model) stopActivityStopwatch() Model {
	m.agent.Stopwatch = applyActivityStopwatchCmd(m.agent.Stopwatch, m.agent.Stopwatch.Stop())
	return m
}
