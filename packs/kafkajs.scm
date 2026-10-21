; kafkajs consumers and producers, and NestJS microservice handlers.
; Docs: kafka.js.org/docs/consuming, docs.nestjs.com/microservices/basics

; consumer.subscribe({ topic: "orders" }) or { topics: ["a", "b"] }
(call_expression
  function: (member_expression property: (property_identifier) @subscribe)
  arguments: (arguments . (object
    (pair key: (property_identifier) @topic_key value: (_) @topic)))
  (#eq? @subscribe "subscribe")
  (#match? @topic_key "^topics?$")) @consumer

; producer.send({ topic: "orders", messages })
(call_expression
  function: (member_expression property: (property_identifier) @send)
  arguments: (arguments . (object
    (pair key: (property_identifier) @topic_key value: (_) @topic)))
  (#match? @send "^(send|sendBatch)$")
  (#eq? @topic_key "topic")) @producer

; @EventPattern("orders") / @MessagePattern("orders") handlers
(decorator
  (call_expression
    function: (identifier) @pattern
    arguments: (arguments . (_) @topic))
  (#match? @pattern "^(EventPattern|MessagePattern)$")) @consumer

; client.emit("orders", data) on a NestJS ClientProxy or ClientKafka
(call_expression
  function: (member_expression
    object: [(identifier) @object (member_expression property: (property_identifier) @object)]
    property: (property_identifier) @send)
  arguments: (arguments . (string) @topic)
  (#match? @object "(?i)(client|kafka|proxy|bus)$")
  (#match? @send "^(emit|send)$")) @producer
