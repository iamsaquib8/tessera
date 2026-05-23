package sample;

import java.util.Optional;

public class UserService {
    private final UserRepository repository;

    public UserService(UserRepository repository) {
        this.repository = repository;
    }

    public Optional<User> findById(String id) {
        return repository.loadUser(id).map(this::hydrate);
    }

    private User hydrate(User user) {
        return user.withTrimmedName();
    }

    public String renderUser(String id) {
        return findById(id).map(User::display).orElse("(missing)");
    }
}
