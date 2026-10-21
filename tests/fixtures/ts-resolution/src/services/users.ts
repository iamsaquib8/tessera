import createRepo, { UserRepo } from "@app/data/repo";
import * as domain from "@app/domain";
import { Injectable } from "./di";

@Injectable()
export class UserService {
  constructor(private readonly repo: UserRepo) {}

  name(id: string): string {
    return this.repo.require(id).display();
  }
}

export function makeService(): UserService {
  const repo: UserRepo = createRepo();
  const other = new UserRepo();
  other.get("x");
  return new UserService(repo);
}

export function isUser(value: unknown): value is domain.User {
  return value instanceof domain.User;
}

const shared = new UserRepo();

export function useShared() {
  return shared.get("x");
}

export class Shop {
  constructor(private readonly service: UserService) {}

  label(id: string) {
    const svc = this.service;
    return svc.name(id);
  }

  direct(id: string) {
    return this.service.name(id);
  }
}
