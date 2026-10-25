import { gql, useQuery } from '@apollo/client';

export const GET_USER = gql`
  query GetUser($id: ID!) {
    user(id: $id) {
      id
      name
    }
  }
`;

export const ADD_USER = gql`mutation AddUser { createUser(name: "x") { id } }`;

export function useUser(id: string) {
  return useQuery(GET_USER, { variables: { id } });
}
