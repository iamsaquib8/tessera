package com.acme.domain;

import java.util.List;

public class User implements Entity, Comparable<User> {
    private final String id;
    private List<String> roles;

    public User(String id, String... roles) {
        this.id = id;
        this.roles = List.of(roles);
    }

    @Override
    public String id() {
        return id;
    }

    @Override
    public int compareTo(User other) {
        return id.compareTo(other.id());
    }
}
