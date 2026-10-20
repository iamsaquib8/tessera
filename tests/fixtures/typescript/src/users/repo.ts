import { formatName } from "../util/format";

export class UserRepo {
  private cache = new Map<string, string>();

  findById(id: string): string {
    return formatName(this.lookup(id));
  }

  private lookup(id: string): string {
    return this.cache.get(id) ?? id;
  }
}

export function loadUser(id: string): string {
  const repo = new UserRepo();
  return repo.findById(id);
}
