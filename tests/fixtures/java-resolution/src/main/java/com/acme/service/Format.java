package com.acme.service;

import com.acme.domain.User;

final class Format {
    static String label(User user) {
        return "[" + user.id() + "]";
    }

    static String shout(String text) {
        return text.toUpperCase();
    }
}
