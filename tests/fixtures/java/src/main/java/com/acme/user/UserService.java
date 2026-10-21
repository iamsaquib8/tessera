package com.acme.user;

import com.acme.util.Strings;

public class UserService {
    private final UserRepository repo;

    public UserService(UserRepository repo) {
        this.repo = repo;
    }

    public String load(String id) {
        return Strings.trim(repo.findById(id));
    }
}
