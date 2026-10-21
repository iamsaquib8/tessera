package shop;

class OrderRepository {
  String byId = "SELECT id, total FROM orders WHERE id = ?";
}
