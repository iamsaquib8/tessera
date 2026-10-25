package main

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /users/{id}", getUser)
	mux.HandleFunc("/health", health)
	mux.Handle("/metrics", metrics())
	http.HandleFunc("/legacy/users", legacyUsers)

	r := chi.NewRouter()
	r.Get("/carts/{cartID}", getCart)
	r.Route("/api", func(r chi.Router) {
		r.Post("/carts", createCart)
	})

	cache := newCache()
	cache.Get("/legacy/users")
	cache.Set("/legacy/users", "x")
	http.ListenAndServe(":8080", mux)
}

func getUser(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	w.Write([]byte(r.URL.Query().Get("x") + id))
}

func health(w http.ResponseWriter, r *http.Request)      {}
func legacyUsers(w http.ResponseWriter, r *http.Request) {}
func getCart(w http.ResponseWriter, r *http.Request)     {}
func createCart(w http.ResponseWriter, r *http.Request)  {}
func metrics() http.Handler                              { return nil }
