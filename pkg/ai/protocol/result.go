package protocol

// TurnResult is a completed provider turn with optional reasoning output.
type TurnResult struct {
	Thinking   string
	Content    string
	Usage      TurnUsage
	ToolCalls  []ToolCall
	StopReason StopReason
}

// TurnStream receives incremental thinking and response text during a turn.
type TurnStream struct {
	OnThinking func(chunk string)
	OnContent  func(chunk string)
}

func (s *TurnStream) emitThinking(chunk string) {
	if s == nil || s.OnThinking == nil || chunk == "" {
		return
	}
	s.OnThinking(chunk)
}

func (s *TurnStream) emitContent(chunk string) {
	if s == nil || s.OnContent == nil || chunk == "" {
		return
	}
	s.OnContent(chunk)
}

// EmitThinking forwards a thinking chunk to stream callbacks.
func (s *TurnStream) EmitThinking(chunk string) { s.emitThinking(chunk) }

// EmitContent forwards a content chunk to stream callbacks.
func (s *TurnStream) EmitContent(chunk string) { s.emitContent(chunk) }
