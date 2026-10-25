package frontend

import (
	"context"
	"fmt"
	"net/http"
	"strings"
)

const usersURL = "http://users:8080"

func user(id string) (*http.Response, error) {
	return http.Get(usersURL + "/users/" + id)
}

func createCart(ctx context.Context) (*http.Request, error) {
	return http.NewRequestWithContext(ctx, http.MethodPost, "http://users:8080/api/carts", strings.NewReader("{}"))
}

func order(client *http.Client, id int) (*http.Response, error) {
	return client.Get(fmt.Sprintf("http://orders/orders/%d", id))
}

func deleteItem(id string) (*http.Request, error) {
	return http.NewRequest("DELETE", "http://orders/admin/items/"+id, nil)
}

func health() (*http.Response, error) {
	return http.Head("http://users:8080/health")
}

func lookalike(headers http.Header, cache map[string]string) string {
	return headers.Get("/metrics") + cache["/legacy/users"]
}
