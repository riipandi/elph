// Package agent implements the framework-neutral coding-agent turn loop.
//
// RunTurn streams Event values on a channel. TUI adapters in
// internal/renderer/agent_bridge.go translate those events into tea.Msg.
package agent
