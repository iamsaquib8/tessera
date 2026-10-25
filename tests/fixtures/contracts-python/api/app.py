import functools

from fastapi import APIRouter, FastAPI

app = FastAPI()
router = APIRouter(prefix="/api/orders")
settings = {"/api/status": True}


@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {"id": user_id, "debug": settings.get("/api/status", False)}


@router.post("/")
async def create_order(order: dict):
    return order


@router.get("/{order_id}")
async def get_order(order_id: int):
    return {"id": order_id}


@functools.lru_cache(maxsize=None)
def cached():
    return settings.get("/users/1", None)


@settings.get("/api/status")
def not_a_route():
    return None


app.include_router(router)
