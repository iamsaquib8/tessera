import React from "react";
import { Avatar } from "./Avatar";

type Props = { name: string };

export const UserCard = ({ name }: Props) => (
  <div>
    <Avatar name={name} />
  </div>
);

export const MemoCard = React.memo(UserCard);
