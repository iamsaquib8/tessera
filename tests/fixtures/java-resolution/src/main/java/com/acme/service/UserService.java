package com.acme.service;

import static com.acme.service.Format.label;
import static com.acme.service.Format.*;

import com.acme.domain.User;
import com.acme.repo.UserRepository;

@Service
public class UserService extends BaseService {
    private final UserRepository repo;

    @Autowired
    public UserService(UserRepository repo) {
        super("users");
        this.repo = repo;
    }

    public String describe(String id) {
        User user = repo.require(id);
        var copy = new User(user.id());
        return label(copy) + shout(user.id());
    }

    public <T extends User> T keep(T value) {
        return value;
    }
}
