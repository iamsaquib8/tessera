import { loadUser } from "../users/repo";
import type { Request } from "./types";

export const handler = (req: Request) => loadUser(req.id);
