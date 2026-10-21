package com.acme.service;

import com.acme.repo.UserRepository;
import org.junit.jupiter.api.Test;

class UserServiceTest {
    @Test
    void describesAUser() {
        UserService service = new UserService(new UserRepository());
        service.describe("1");
    }

    @org.junit.jupiter.api.Disabled
    void skipped() {}
}
