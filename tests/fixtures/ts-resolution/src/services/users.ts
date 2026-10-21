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
