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
        "CartService/AddItem: cart/CartService.cs -> ",
        "CartService/GetCart: cart/CartService.cs -> frontend/rpc.go",
        "CurrencyService/Convert: currency/server.js -> frontend/rpc.go",
        "CurrencyService/GetSupportedCurrencies: currency/server.js -> ",
        "ProductCatalogService/ListProducts:  -> recommendation/client.py",
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
    let consumer = &cart["consumers"][0];
    assert_eq!(consumer["symbol"], "frontendServer.getCart");
    assert_eq!(consumer["service"], "frontend");
    assert_eq!(consumer["line"], 10);
}
