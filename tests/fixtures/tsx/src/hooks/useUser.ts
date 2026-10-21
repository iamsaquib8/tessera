import { useState } from "react";

export function useUser(id: string) {
  const [user] = useState(id);
  return user;
}
