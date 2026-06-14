package providertests

import (
	"encoding/json"
	"net/http"
)

func writeJSONResponse(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func writeEventStreamResponse(w http.ResponseWriter) {
	w.Header().Set("Content-Type", "text/event-stream")
}
