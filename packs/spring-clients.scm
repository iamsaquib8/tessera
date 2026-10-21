; Spring HTTP clients: Feign interfaces, RestTemplate, WebClient, RestClient.
; Docs: docs.spring.io/spring-cloud-openfeign, docs.spring.io/spring-framework/reference/integration/rest-clients.html

; @FeignClient(name = "x", path = "/api") interface X { @GetMapping("/y") Y y(); }
(interface_declaration
  (modifiers
    (annotation
      name: (identifier) @feign
      arguments: (annotation_argument_list) @prefix_path))
  body: (interface_body
    (method_declaration
      (modifiers
        (annotation name: (identifier) @method arguments: (annotation_argument_list) @args)) @client))
  (#eq? @feign "FeignClient")
  (#match? @method "^(Get|Post|Put|Patch|Delete|Request)Mapping$"))

; restTemplate.getForObject("http://svc/api/x", X.class)
(method_invocation
  object: (_) @object
  name: (identifier) @method
  arguments: (argument_list . [(string_literal) (binary_expression)] @path)
  (#match? @object "(?i)rest(template|operations)?$")
  (#match? @method "^(getForObject|getForEntity|postForObject|postForEntity|postForLocation|put|delete|patchForObject|exchange)$")) @client

; webClient.get().uri("/api/x"), restClient.post().uri(...)
(method_invocation
  object: (method_invocation name: (identifier) @method)
  name: (identifier) @uri
  arguments: (argument_list . [(string_literal) (binary_expression)] @path)
  (#eq? @uri "uri")
  (#match? @method "^(get|post|put|patch|delete|head|options)$")) @client

; Tests: mockMvc.perform(get("/owners/{id}", 1)) with MockMvcRequestBuilders
; imported statically.
(method_invocation
  !object
  name: (identifier) @method
  arguments: (argument_list . [(string_literal) (binary_expression)] @path)
  (#match? @method "^(get|post|put|patch|delete)$")) @client
