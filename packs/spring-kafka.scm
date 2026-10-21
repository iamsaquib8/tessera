; Spring for Apache Kafka: @KafkaListener consumers and KafkaTemplate
; producers. Docs: docs.spring.io/spring-kafka/reference/kafka/receiving-messages/listener-annotation.html

(method_declaration
  (modifiers
    (annotation
      name: (identifier) @listener
      arguments: (annotation_argument_list) @topic_args))
  (#match? @listener "^(KafkaListener|RabbitListener|JmsListener|SqsListener)$")) @consumer

(class_declaration
  (modifiers
    (annotation
      name: (identifier) @listener
      arguments: (annotation_argument_list) @topic_args))
  (#eq? @listener "KafkaListener")) @consumer

; kafkaTemplate.send("orders", key, value), template.send(TOPIC, value)
(method_invocation
  object: (_) @object
  name: (identifier) @send
  arguments: (argument_list . (_) @topic)
  (#match? @object "(?i)(kafka|template)$")
  (#match? @send "^(send|sendDefault|convertAndSend)$")) @producer
