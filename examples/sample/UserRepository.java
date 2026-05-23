package sample;

import java.util.Optional;

public class UserRepository {
    public Optional<User> loadUser(String id) {
        return Optional.of(new User(id, "Ada"));
    }
}
