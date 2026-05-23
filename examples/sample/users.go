package sample

import "strings"

type User struct {
	ID   string
	Name string
}

func FindByID(id string) User {
	return hydrateUser(loadUser(id))
}

func loadUser(id string) User {
	return User{ID: id, Name: "Ada"}
}

func hydrateUser(u User) User {
	u.Name = strings.TrimSpace(u.Name)
	return u
}

func (u User) Render() string {
	return u.Name + " (" + u.ID + ")"
}

func RenderUser(id string) string {
	u := FindByID(id)
	return u.Render()
}
