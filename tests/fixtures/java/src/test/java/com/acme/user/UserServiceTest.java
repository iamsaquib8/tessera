package com.acme.user;

import org.junit.jupiter.api.Test;

class UserServiceTest {
    @Test
    void loads() {
        new UserService(new UserRepository()).load("a");
    }
}
