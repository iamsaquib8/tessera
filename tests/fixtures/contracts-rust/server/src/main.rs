use actix_web::{delete, get, head, patch, post, put, web, App, HttpServer, Responder};

#[get("/users/{id}")]
async fn get_user(path: web::Path<u32>) -> impl Responder {
    "user"
}

#[post("/orders")]
#[inline]
async fn create_order() -> impl Responder {
    "order"
}

#[put("/users/{id}")]
async fn update_user() -> impl Responder {
    "updated"
}

#[patch("/users/{id}")]
async fn patch_user() -> impl Responder {
    "patched"
}

#[delete("/users/{id}")]
async fn delete_user() -> impl Responder {
    "deleted"
}

#[head("/health")]
async fn check_health_head() -> impl Responder {
    "head"
}

async fn health() -> impl Responder {
    "ok"
}

async fn create_item() -> impl Responder {
    "item"
}

async fn delete_item() -> impl Responder {
    "deleted item"
}

// Look-alikes that must not match as route facts:
#[inline]
fn helper() {
    let mut fake = FakeRouter;
    fake.route("/not-a-web-route", 42);
}

struct FakeRouter;
impl FakeRouter {
    fn route(&mut self, _path: &str, _val: i32) {}
}

pub fn create_app() {
    App::new()
        .service(get_user)
        .service(create_order)
        .service(update_user)
        .service(patch_user)
        .service(delete_user)
        .service(check_health_head)
        .route("/health", web::get().to(health))
        .service(
            web::scope("/api")
                .route("/items", web::post().to(create_item))
                .route("/items/{id}", web::delete().to(delete_item)),
        );
}
