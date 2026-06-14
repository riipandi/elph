package renderer

import (
	"time"

	"github.com/riipandi/elph/internal/constants"
)

// messageRenderCache stores a rendered message block for reuse across viewport
// rebuilds. Invalidated automatically when width, source length, or streaming
// state changes.
type messageRenderCache struct {
	width             int
	sourceLen         int
	streaming         bool
	expanded          bool
	detailStatus      constants.DetailStatus
	atUnix            int64
	showStatusPreview bool
	spinnerFrame      int
	output            string
}

func (c messageRenderCache) hit(width int, streaming bool, sourceLen int, expanded bool, detailStatus constants.DetailStatus, at time.Time, opts collapsibleRenderOpts) bool {
	return c.output != "" &&
		c.width == width &&
		c.streaming == streaming &&
		c.sourceLen == sourceLen &&
		c.expanded == expanded &&
		c.detailStatus == detailStatus &&
		c.atUnix == messageAtUnix(at) &&
		c.showStatusPreview == opts.showStatusPreview &&
		c.spinnerFrame == opts.spinnerFrame
}
