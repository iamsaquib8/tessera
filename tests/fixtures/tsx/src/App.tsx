import { UserCard } from "./components/UserCard";
import { useUser } from "./hooks/useUser";

export default function App() {
  const user = useUser("a");
  return <UserCard name={user} />;
}
