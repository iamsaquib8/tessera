package shop;

import shop.ProductCatalogServiceGrpc.ProductCatalogServiceBlockingStub;

public class Recommender {
  private final ProductCatalogServiceBlockingStub catalog;

  Recommender(ProductCatalogServiceBlockingStub catalog) {
    this.catalog = catalog;
  }

  int count() {
    return catalog.listProducts(Empty.getDefaultInstance()).getProductsCount();
  }
}
