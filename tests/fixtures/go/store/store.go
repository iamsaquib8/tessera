package store

type Store struct {
	items map[string]string
}

func New() *Store {
	return &Store{items: map[string]string{}}
}

func (s *Store) Get(id string) string {
	return s.items[id]
}

func Load(id string) string {
	return New().Get(id)
}
