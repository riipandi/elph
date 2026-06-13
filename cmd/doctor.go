package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

var doctorCmd = &cobra.Command{
	Use:   "doctor",
	Short: "Validate elph configuration",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("doctor: not yet implemented")
	},
}
