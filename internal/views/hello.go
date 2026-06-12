package views

import (
	"fmt"
	"os"

	tui "github.com/grindlemire/go-tui"
)

func ViewHello() {
	app, err := tui.NewApp(
		tui.WithRootComponent(Hello()),
	)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
	defer app.Close()

	if err := app.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}
