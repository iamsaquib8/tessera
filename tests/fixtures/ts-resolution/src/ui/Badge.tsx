import React, { memo } from "react";
import { User } from "@app/domain";

type Props = { user: User };

export const Badge = memo(({ user }: Props) => <span>{user.display()}</span>);

export default ({ user }: Props) => <Badge user={user} />;
