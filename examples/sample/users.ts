export type User = {
  id: string;
  name: string;
};

export function findById(id: string): User {
  return hydrateUser(loadUser(id));
}

function loadUser(id: string): User {
  return { id, name: "Ada" };
}

function hydrateUser(user: User): User {
  return { ...user, name: user.name.trim() };
}

export function renderUser(id: string): string {
  const user = findById(id);
  return `${user.name} (${user.id})`;
}
