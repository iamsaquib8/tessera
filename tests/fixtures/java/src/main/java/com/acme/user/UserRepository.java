package com.acme.user;

public class UserRepository {
    public String findById(String id) {
        return id;
    }

    public String findById(String id, boolean cached) {
        return findById(id);
    }
}
