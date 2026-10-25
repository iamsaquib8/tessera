import httpx
import requests

BASE = "http://api:8000"
cache = {}


def fetch_user(user_id):
    return requests.get(f"{BASE}/users/{user_id}", timeout=5)


def create_order(order):
    return httpx.post("http://api:8000/api/orders/", json=order)


async def get_order(order_id):
    async with httpx.AsyncClient(base_url=BASE) as client:
        return await client.get(f"/api/orders/{order_id}")


def update_cart(session, cart_id):
    return session.post("http://web:5000/carts/" + str(cart_id))


def delete_item(item_id):
    return httpx.Client().delete(f"http://web:5000/items/{item_id}")


def accounts():
    return requests.get("http://shop/accounts/7/")


def lookalike():
    return cache.get("/carts", None) or os_environ_get("/api/status")


def os_environ_get(key):
    return key
