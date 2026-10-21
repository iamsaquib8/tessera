package shop;

import jakarta.persistence.*;

@Entity
@Table(name = "orders")
class Order {
  @Id Long id;
}

@Entity
class OrderLine {
  @Id Long id;
}
