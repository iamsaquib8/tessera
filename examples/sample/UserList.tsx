import { useEffect, useState } from "react";

type User = { id: string; name: string };

export function UserAvatar({ user }: { user: User }) {
  return <div className="avatar">{user.name[0]}</div>;
}

export function UserCard({ user }: { user: User }) {
  return (
    <article className="card">
      <UserAvatar user={user} />
      <span>{user.name}</span>
    </article>
  );
}

export function UserList({ ids }: { ids: string[] }) {
  const [users, setUsers] = useState<User[]>([]);

  useEffect(() => {
    Promise.all(ids.map((id) => fetchUser(id))).then(setUsers);
  }, [ids]);

  return (
    <section>
      {users.map((user) => (
        <UserCard key={user.id} user={user} />
      ))}
    </section>
  );
}

async function fetchUser(id: string): Promise<User> {
  return { id, name: "Ada" };
}
