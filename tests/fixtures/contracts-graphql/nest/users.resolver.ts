import { Mutation, Query, Resolver } from '@nestjs/graphql';
import { User } from './user.model';

@Resolver(() => User)
export class UsersResolver {
  @Query(() => [User])
  users(): User[] {
    return [];
  }

  @Mutation(() => User, { name: 'createUser' })
  addUser(name: string): User {
    return { id: '1', name };
  }
}
