package renderer

// messageRenderCache stores a rendered message block for reuse across viewport
// rebuilds. Invalidated automatically when width, source length, or streaming
// state changes.
type messageRenderCache struct {
	width     int
	sourceLen int
	streaming bool
	output    string
}

func (c messageRenderCache) hit(width int, streaming bool, sourceLen int) bool {
	return c.output != "" &&
		c.width == width &&
		c.streaming == streaming &&
		c.sourceLen == sourceLen
}
