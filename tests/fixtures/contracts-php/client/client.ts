export async function getUser(id: string) {
  return fetch(`/users/${id}`);
}

export async function createOrder(data: any) {
  return fetch('/orders', {
    method: 'POST',
    body: JSON.stringify(data),
  });
}

export async function updateItem(id: string, data: any) {
  return fetch(`/items/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

export async function patchUser(id: string, data: any) {
  return fetch(`/users/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(data),
  });
}

export async function checkHealth() {
  return fetch('/health', {
    method: 'OPTIONS',
  });
}

export async function deleteOrder(id: string) {
  return fetch(`/api/orders/${id}`, {
    method: 'DELETE',
  });
}

export async function getStatus() {
  return fetch('/api/status');
}

export async function ping() {
  return fetch('/v2/ping');
}
