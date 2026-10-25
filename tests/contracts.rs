mod support;

use std::collections::BTreeSet;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn map(root: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "--json", "map"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// `name: provider files -> consumer files` for every contract.
fn summary(value: &Value) -> BTreeSet<String> {
    let files = |sites: &Value| -> String {
        let mut paths: Vec<String> = sites
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["path"].as_str().unwrap().to_string())
            .collect();
        paths.sort();
        paths.dedup();
        paths.join(",")
    };
    value["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            format!(
                "{}: {} -> {}",
                c["name"].as_str().unwrap(),
                files(&c["providers"]),
                files(&c["consumers"])
            )
        })
        .collect()
}

fn fixture(name: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir(name), temp.path());
    temp
}

#[test]
fn grpc_clients_link_to_servers_in_five_languages() {
    let temp = fixture("contracts-grpc");
    let result = map(temp.path(), &["rpc"]);
    let got = summary(&result);
    for expected in [
        "CartService/AddItem: cart/CartService.cs -> gateway/cart.gateway.ts",
        "CartService/GetCart: cart/CartService.cs -> cart/CartServiceTests.cs,currency-cpp/server.cpp,frontend/rpc.go",
        "CurrencyService/Convert: currency-cpp/server.cpp,currency/server.js -> frontend/rpc.go",
        "CurrencyService/GetSupportedCurrencies: currency/server.js -> frontend/rpc.go",
        "ProductCatalogService/ListProducts:  -> ad/src/main/java/shop/Recommender.java,recommendation/client.py",
        "EmailService/SendOrderConfirmation: email/email_server.py -> checkout/main.go",
        "CheckoutService/PlaceOrder: checkout/main.go -> ",
        "AdService/GetAds: ad/src/main/java/shop/AdService.java -> ad/src/main/java/shop/AdServiceClient.java",
    ] {
        assert!(got.contains(expected), "missing {expected:?} in {got:#?}");
    }
    assert_eq!(got.len(), 8, "{got:#?}");
    let text = result.to_string();
    assert!(!text.contains("demo_grpc.pb.go"), "generated code: {text}");
    assert!(!text.contains("demo_pb2_grpc.py"), "generated code: {text}");
}

#[test]
fn every_site_names_its_handler_service_and_definition() {
    let temp = fixture("contracts-grpc");
    let result = map(temp.path(), &["rpc", "GetCart"]);
    let contracts = result["contracts"].as_array().unwrap();
    assert_eq!(contracts.len(), 1, "{result}");
    let cart = &contracts[0];
    assert_eq!(cart["definitions"][0]["path"], "protos/demo.proto");
    assert_eq!(cart["definitions"][0]["line"], 7);
    let provider = &cart["providers"][0];
    assert_eq!(provider["symbol"], "CartService.GetCart");
    assert_eq!(provider["service"], "cart");
    assert_eq!(provider["confidence"], "high");
    let consumer = cart["consumers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["path"] == "frontend/rpc.go")
        .unwrap();
    assert_eq!(consumer["symbol"], "frontendServer.getCart");
    assert_eq!(consumer["service"], "frontend");
    assert_eq!(consumer["line"], 10);
}

#[test]
fn http_routes_link_to_their_clients_across_frameworks() {
    let temp = fixture("contracts-http");
    let result = map(temp.path(), &["route"]);
    let got = summary(&result);
    for expected in [
        "GET /api/users/{}: api/src/app.js -> web/src/client.ts",
        "POST /api/users: api/src/app.js -> web/src/client.ts",
        "GET /api/orders/{}: nest/src/orders.controller.ts -> web/src/client.ts",
        "POST /api/orders: nest/src/orders.controller.ts -> ",
        "ANY /api/health: web/pages/api/health.ts -> ",
        "GET /api/items/{}: web/app/api/items/[id]/route.ts -> web/src/client.ts",
        "GET /owners/{}: owners/src/main/java/petclinic/OwnerController.java -> billing/src/main/java/billing/Invoices.java,billing/src/main/java/billing/OwnerClient.java,owners/src/test/java/petclinic/OwnerControllerTests.java",
        "POST /owners/new: owners/src/main/java/petclinic/OwnerController.java -> ",
        "GET /owners: owners/src/main/java/petclinic/OwnerController.java -> ",
        "GET /repos:  -> web/src/client.ts",
    ] {
        assert!(got.contains(expected), "missing {expected:?} in {got:#?}");
    }
    assert_eq!(got.len(), 10, "{got:#?}");
}

#[test]
fn route_sites_carry_handlers_confidence_and_spec() {
    let temp = fixture("contracts-http");
    let result = map(temp.path(), &["route", "/api/users/{}"]);
    let users = &result["contracts"][0];
    assert_eq!(users["name"], "GET /api/users/{}", "{result}");
    assert_eq!(users["definitions"][0]["path"], "api/openapi.yaml");
    assert_eq!(users["providers"][0]["symbol"], "getUser");
    let confidences: Vec<(u64, &str)> = users["consumers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["line"].as_u64().unwrap(),
                s["confidence"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(confidences, vec![(8, "low"), (9, "medium")]);

    let result = map(temp.path(), &["route", "/api/orders/{}"]);
    let orders = &result["contracts"][0];
    assert_eq!(
        orders["providers"][0]["symbol"], "OrdersController.findOne",
        "{result}"
    );
    assert_eq!(orders["consumers"][0]["symbol"], "order");
}

#[test]
fn topics_link_producers_and_consumers_across_languages() {
    let temp = fixture("contracts-data");
    let got = summary(&map(temp.path(), &["topic"]));
    for expected in [
        "orders.created: orders/src/main/java/shop/OrderEvents.java -> notify/src/consumer.ts",
        "payments.settled: notify/src/consumer.ts -> orders/src/main/java/shop/OrderEvents.java",
        "payments.failed:  -> notify/src/handlers.ts,orders/src/main/java/shop/OrderEvents.java",
    ] {
        assert!(got.contains(expected), "missing {expected:?} in {got:#?}");
    }
    assert_eq!(got.len(), 3, "{got:#?}");
}

#[test]
fn tables_link_migrations_models_and_queries() {
    let temp = fixture("contracts-data");
    let result = map(temp.path(), &["table"]);
    let tables: BTreeSet<String> = result["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let paths = |side: &str| -> String {
                let mut p: Vec<String> = c[side]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| {
                        s["path"]
                            .as_str()
                            .unwrap()
                            .rsplit('/')
                            .next()
                            .unwrap()
                            .to_string()
                    })
                    .collect();
                p.sort();
                p.dedup();
                p.join(",")
            };
            format!(
                "{}: {} | {} | {}",
                c["name"].as_str().unwrap(),
                paths("definitions"),
                paths("providers"),
                paths("consumers")
            )
        })
        .collect();
    for expected in [
        "orders: V1__orders.sql,V2__orders_status.sql | Order.java | OrderRepository.java",
        "order_line: V1__orders.sql | Order.java | ",
        "customers:  | schema.prisma | db.ts",
        "notifications:  | handlers.ts | ",
    ] {
        assert!(
            tables.contains(expected),
            "missing {expected:?} in {tables:#?}"
        );
    }
    assert_eq!(tables.len(), 4, "{tables:#?}");
}

fn tessera(root: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "--json"])
        .args(args)
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn path_crosses_a_service_boundary_through_a_contract() {
    let temp = fixture("contracts-grpc");
    // Go's `GetSupportedCurrencies` call and Node's `getSupportedCurrencies`
    // handler share no name; only the contract links them.
    let result = tessera(
        temp.path(),
        &[
            "path",
            "frontendServer.currencies",
            "getSupportedCurrencies",
        ],
    );
    assert_eq!(result["found"], true, "{result}");
    let hops: Vec<&str> = result["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        hops,
        vec!["frontend/rpc.go", "currency/server.js"],
        "{result}"
    );
}

#[test]
fn map_service_shows_what_a_service_calls_and_serves() {
    let temp = fixture("contracts-grpc");
    let result = map(temp.path(), &["service", "checkout"]);
    let service = &result["services"][0];
    assert_eq!(service["name"], "checkout", "{result}");
    let calls: Vec<String> = service["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            format!(
                "{} -> {}",
                l["contract"].as_str().unwrap(),
                l["services"][0].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        calls,
        vec!["EmailService/SendOrderConfirmation -> email"],
        "{result}"
    );
    let serves: Vec<&str> = service["serves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["contract"].as_str().unwrap())
        .collect();
    assert_eq!(serves, vec!["CheckoutService/PlaceOrder"], "{result}");
}

#[test]
fn a_workspace_links_contracts_across_repositories() {
    let temp = TempDir::new().unwrap();
    let http = fixture_dir("contracts-http");
    for repo in ["api", "web"] {
        let dir = temp.path().join(repo);
        copy_dir(&http.join(repo), &dir);
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
    }
    std::fs::write(
        temp.path().join("tessera.workspace.toml"),
        "# Repositories mapped together\n[[repo]]\npath = \"api\"\n\n[[repo]]\npath = \"web\"\n",
    )
    .unwrap();
    let got = summary(&map(&temp.path().join("web"), &["route"]));
    assert!(
        got.contains("GET /api/users/{}: api/src/app.js -> web/src/client.ts"),
        "{got:#?}"
    );
    assert!(
        got.contains("POST /api/users: api/src/app.js -> web/src/client.ts"),
        "{got:#?}"
    );
}

#[test]
fn the_report_in_a_workspace_names_the_contracts_a_change_touches() {
    let temp = TempDir::new().unwrap();
    let http = fixture_dir("contracts-http");
    for repo in ["api", "web"] {
        let dir = temp.path().join(repo);
        copy_dir(&http.join(repo), &dir);
        for args in [
            &["init", "-q"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-qm",
                "base",
            ],
        ] {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .status()
                .unwrap()
                .success());
        }
    }
    std::fs::write(
        temp.path().join("tessera.workspace.toml"),
        "[[repo]]\npath = \"api\"\n\n[[repo]]\npath = \"web\"\n",
    )
    .unwrap();
    let client = temp.path().join("web/src/client.ts");
    let text = std::fs::read_to_string(&client).unwrap();
    std::fs::write(&client, format!("{text}\n// touched\n")).unwrap();
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path().join("web"))
        .args(["--quiet", "verify", "--changed", "--format", "markdown"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("`route GET /api/users/{}`"), "{report}");
}

#[test]
fn a_route_is_served_by_its_handler_not_the_next_function() {
    let temp = TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("controller.js"),
        "exports.a = function a(req, res) { res.send('a'); };\nexports.b = function b(req, res) { res.send('b'); };\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("app.js"),
        "const app = require('express')();\nconst controller = require('./controller');\n\
         app.get('/a', controller.a);\napp.post('/b', controller.b);\n\n\
         function formatDate(d) {\n  return d;\n}\n",
    )
    .unwrap();
    let value = map(temp.path(), &["route"]);
    let handlers: Vec<String> = value["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["providers"].as_array().unwrap().iter())
        .map(|s| s["symbol"].as_str().unwrap_or("-").to_string())
        .collect();
    assert_eq!(handlers.len(), 2, "{value}");
    assert!(
        !handlers.iter().any(|h| h.contains("formatDate")),
        "{handlers:?}"
    );
}

#[test]
fn routes_on_a_mounted_router_carry_the_mount_prefix() {
    let temp = TempDir::new().unwrap();
    let files = [
        (
            "app.js",
            "const express = require('express');\n\
             const users = require('./routes/users');\n\
             const { ordersRouter } = require('./routes/orders');\n\
             const app = express();\n\
             const adminRouter = express.Router();\n\
             adminRouter.get('/stats', (req, res) => res.send({}));\n\
             app.use('/api/users', users);\n\
             app.use('/api', ordersRouter);\n\
             app.use('/admin', adminRouter);\n\
             app.get('/health', (req, res) => res.send('ok'));\n",
        ),
        (
            "routes/users.js",
            "const router = require('express').Router();\n\
             router.get('/:id', (req, res) => res.send(req.params.id));\n\
             module.exports = router;\n",
        ),
        (
            "routes/orders.js",
            "const ordersRouter = require('express').Router();\n\
             ordersRouter.post('/orders', (req, res) => res.send(req.body));\n\
             module.exports = { ordersRouter };\n",
        ),
        (
            "web/client.js",
            "export const user = (id) => fetch(`/api/users/${id}`);\n\
             export const order = (o) => fetch('/api/orders', { method: 'POST', body: o });\n\
             export const stats = () => fetch('/admin/stats');\n",
        ),
    ];
    for (path, content) in files {
        let file = temp.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }
    let got = summary(&map(temp.path(), &["route"]));
    for expected in [
        "GET /api/users/{}: routes/users.js -> web/client.js",
        "POST /api/orders: routes/orders.js -> web/client.js",
        "GET /admin/stats: app.js -> web/client.js",
        "GET /health: app.js -> ",
    ] {
        assert!(got.contains(expected), "missing {expected}: {got:#?}");
    }
    assert!(
        !got.iter()
            .any(|c| c.starts_with("GET /{}:") || c.starts_with("GET /stats:")),
        "unprefixed copies of mounted routes: {got:#?}"
    );
}
