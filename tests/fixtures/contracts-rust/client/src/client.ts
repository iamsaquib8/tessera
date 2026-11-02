export async function getUser(id: string) {
  return fetch(`/users/${id}`);
}

export async function createOrder(data: any) {
  return fetch('/orders', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function updateUser(id: string, data: any) {
  return fetch(`/users/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function checkHealth() {
  return fetch('/health');
}

export async function createItem(item: any) {
  return fetch('/api/items', {
    method: 'POST',
    body: JSON.stringify(item),
  });
}

export async function deleteItem(id: string) {
  return fetch(`/api/items/${id}`, {
    method: 'DELETE',
  });
}
