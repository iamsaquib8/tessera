package com.acme.domain;

public final class Errors {
    public static IllegalStateException notFound(String key) {
        return new IllegalStateException("missing " + key);
    }
}
