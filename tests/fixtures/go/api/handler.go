package api

import "example.com/app/store"

func Handle(id string) string {
	return store.Load(id)
}

func Load(id string) string {
	return id
}
