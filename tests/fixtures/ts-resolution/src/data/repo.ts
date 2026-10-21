import { User, notFound } from "../domain";

export abstract class Repo<T> {
  abstract get(id: string): T | undefined;
}

export class UserRepo extends Repo<User> {
  private rows = new Map<string, User>();

  get(id: string): User | undefined {
    return this.rows.get(id);
  }

  require(id: string, strict = true, ...tags: string[]): User {
    const user = this.get(id);
    if (!user) throw notFound(id);
    return user;
  }
}

export default function createRepo(): UserRepo {
  return new UserRepo();
}

export class RepoCache {
  static shared = new RepoCache();

  static build = (size: number) => new RepoCache();

  clear = () => {
    this.reset();
  };

  reset() {}
}

export function warm() {
  RepoCache.build(1).clear();
}
