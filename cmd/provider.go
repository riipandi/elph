package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

var updateProvidersCmd = &cobra.Command{
	Use:   "update-providers",
	Short: "Update AI providers and models",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("update-providers: not yet implemented")
	},
}
