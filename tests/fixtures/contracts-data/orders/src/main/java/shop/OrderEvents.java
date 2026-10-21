package shop;

import org.springframework.kafka.annotation.KafkaListener;
import org.springframework.kafka.core.KafkaTemplate;

class OrderEvents {
  static final String ORDERS = "orders.created";
  private KafkaTemplate<String, String> kafkaTemplate;

  void publish(String order) {
    kafkaTemplate.send(ORDERS, order);
  }

  @KafkaListener(topics = {"payments.settled", "payments.failed"}, groupId = "orders")
  void onPayment(String payment) {
  }
}
