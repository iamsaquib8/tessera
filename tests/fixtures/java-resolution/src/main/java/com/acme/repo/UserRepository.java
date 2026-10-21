package com.acme.repo;

import com.acme.domain.*;
import java.util.HashMap;
import java.util.Map;
import java.util.Optional;

public class UserRepository implements Repository<User, String> {
    private final Map<String, User> rows = new HashMap<>();

    @Override
    public Optional<User> find(String key) {
        return Optional.ofNullable(rows.get(key));
    }

    public User require(String key) {
        return find(key).orElseThrow(() -> Errors.notFound(key));
    }

    public User require(String key, boolean strict) {
        User user = require(key);
        return user;
    }

    @Override
    public void close() {}
}
