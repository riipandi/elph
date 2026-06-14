package provider

// TurnResult is a completed provider turn with optional reasoning output.
type TurnResult struct {
	Thinking string
	Content  string
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
