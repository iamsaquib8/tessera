package com.acme.repo;

import java.util.Optional;

public interface Repository<T, K> extends AutoCloseable {
    Optional<T> find(K key);
}
