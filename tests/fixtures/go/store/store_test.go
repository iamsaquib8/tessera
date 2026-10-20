package store

import "testing"

func TestLoad(t *testing.T) {
	if Load("a") != "" {
		t.Fatal("unexpected")
	}
}
