package com.acme.service;

public abstract class BaseService {
    protected final String name;

    protected BaseService(String name) {
        this.name = name;
    }
}
