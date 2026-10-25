package main

import "os"

func port() string {
	return os.Getenv("PORT")
}
