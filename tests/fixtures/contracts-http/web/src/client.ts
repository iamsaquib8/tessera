import axios from 'axios';
import useSWR from 'swr';

const api = axios.create({ baseURL: '/' });
const lookup = new Map<string, string>();

export async function loadUser(id: string) {
  const plain = await fetch('/api/users/' + id);
  const templated = await fetch(`/api/users/${id}`);
  return [plain, templated];
}

export function saveUser(body: object) {
  return axios.post('/api/users', body);
}

export function order() {
  return api.get('/api/orders/42');
}

export function repos() {
  lookup.get('/api/users/1');
  return fetch('https://api.github.com/repos');
}

export function useItem() {
  return useSWR('/api/items/7', (url: string) => fetch(url));
}
