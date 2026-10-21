package com.acme.web;

import com.acme.user.UserRepository;
import com.acme.user.UserService;

@RestController
public class UserController {
    private final UserService service = new UserService(new UserRepository());

    @GetMapping("/users/{id}")
    public String get(@PathVariable String id) {
        return service.load(id);
    }
}
